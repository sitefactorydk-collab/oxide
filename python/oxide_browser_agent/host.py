"""The browser: started (or reached over CDP) on first use and shared by every tool.

Oxide's own Chromium runs with a throwaway profile and a remote-debugging port on
127.0.0.1, so Browser-Use (``agent.py``) can drive the same browser the granular tools use.
In "cdp" mode the user's running Chrome is used instead: Oxide opens its own tab there and
never closes the user's tabs or the browser.
"""

from __future__ import annotations

import asyncio
import re
import shutil
import socket
import sys
import tempfile
from typing import Any
from urllib.parse import quote_plus, unquote, urlsplit

from .config import Config
from .pagetext import PAGE_TEXT_JS, REF_ATTR

NAV_TIMEOUT_MS = 45_000
ACTION_TIMEOUT_MS = 10_000
SETTLE_MS = 2_500


class BrowserFailure(Exception):
    """A failure the model is told about (the message is meant for it)."""


def _log(msg: str) -> None:
    print(f"[oxide-browser] {msg}", file=sys.stderr, flush=True)


def free_port() -> int:
    s = socket.socket()
    try:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])
    finally:
        s.close()


# Searches go to Google (or the engine in OXIDE_BROWSER_SEARCH_URL). When it answers with a
# check for robots, the search is reported as refused: Oxide never tries to get around such a
# check, and the user can complete it in a visible browser.
GOOGLE_SEARCH = "https://www.google.com/search?hl=en&q={q}"
# Signs of a check for robots instead of results.
BLOCKED_MARKERS = (
    "unusual traffic",
    "not a robot",
    "are you a robot",
    "verify you are human",
    "confirm you're not a robot",
    "captcha",
)
# Answers that mean the page isn't there: the browser searches instead.
GONE_STATUSES = (404, 410)
# Navigation errors that mean the address doesn't exist.
UNKNOWN_HOST = (
    "ERR_NAME_NOT_RESOLVED",
    "ERR_NAME_RESOLUTION_FAILED",
    "ERR_ADDRESS_INVALID",
    "NS_ERROR_UNKNOWN_HOST",
)
# Google's cookie page in the EU: its "Reject all" button (in a few languages), else accept.
CONSENT_REJECT = re.compile(
    r"reject all|afvis alle|alle ablehnen|tout refuser|rechazar todo|rifiuta tutto|"
    r"alles afwijzen|avvisa alla|avvis alle",
    re.I,
)
CONSENT_ACCEPT = re.compile(r"accept all|acceptér alle|accepter alle|alle akzeptieren", re.I)


def is_search_words(text: str) -> bool:
    """Whether what the model passed as a URL is really search words ("opening hours
    natmus")."""
    t = (text or "").strip()
    if not t or re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", t):
        return False
    if re.search(r"\s", t):
        return True
    host = t.split("/")[0]
    return "." not in host and not host.lower().startswith(("localhost", "[::1]"))


def search_url(words: str, template: str = "") -> str:
    return (template or GOOGLE_SEARCH).replace("{q}", quote_plus(words.strip()))


def query_from_url(url: str) -> str:
    """Search words for an address that didn't work: its site and the words in its path."""
    try:
        parts = urlsplit(url)
    except ValueError:
        return url
    host = (parts.hostname or "").lower()
    if host.startswith("www."):
        host = host[4:]
    words = [host] if host else []
    for seg in unquote(parts.path).split("/"):
        seg = re.sub(r"\.(html?|php|aspx?|jsp)$", "", seg, flags=re.I)
        for w in re.split(r"[-_.+\s]+", seg):
            if w and not w.isdigit() and len(w) < 40:
                words.append(w)
    return " ".join(words[:12]) or url


def is_blocked(url: str, text: str) -> bool:
    """Whether a search engine showed a check for robots (or its cookie page) instead of
    results."""
    low = text.lower()
    return "/sorry/" in url or "consent.google." in url or any(m in low for m in BLOCKED_MARKERS)


def host_of(url: str) -> str:
    try:
        return (urlsplit(url).netloc or "").lower()
    except ValueError:
        return ""


def click_problem(error: str) -> str | None:
    """Why Playwright couldn't click, in words for the model, when the reason is one a direct
    click wouldn't fix (something covers the element, it is hidden or disabled). `None`: it
    only timed out waiting for the element to hold still."""
    if "intercepts pointer events" in error:
        return (
            "something else on the page covers it (often a cookie banner or a pop-up). Close "
            "or answer that first (read_page shows its buttons), then click again."
        )
    if "element is not visible" in error:
        return "it isn't visible (maybe in a closed menu). Read the page again."
    if "element is not enabled" in error or "element is disabled" in error:
        return "it is disabled."
    if "not attached to the DOM" in error:
        return "the page changed under it. Read the page again and use the new number."
    return None


def is_unknown_host(message: str) -> bool:
    return any(k in message for k in UNKNOWN_HOST)


def normalize_url(url: str) -> str:
    u = (url or "").strip()
    if not u:
        raise BrowserFailure("navigate needs a URL.")
    low = u.lower()
    if low.startswith(("http://", "https://", "file:", "about:", "data:")):
        return u
    if low.startswith(("javascript:", "chrome:", "chrome-extension:", "view-source:")):
        raise BrowserFailure("Only http, https, file and about: addresses can be opened.")
    if low.startswith(("localhost", "127.0.0.1", "[::1]")):
        return "http://" + u
    return "https://" + u


def first_line(e: BaseException) -> str:
    text = str(e).strip() or e.__class__.__name__
    line = text.splitlines()[0]
    return line[:400]


class BrowserHost:
    def __init__(self, cfg: Config):
        self.cfg = cfg
        self._pw: Any = None
        self._browser: Any = None
        self._context: Any = None
        self._page: Any = None
        self._own_pages: list[Any] = []
        self._profile: str | None = None
        self._start_lock = asyncio.Lock()
        self.cdp_url: str | None = None

    @property
    def started(self) -> bool:
        return self._context is not None

    # ---- Start and stop -----------------------------------------------------------------

    async def start(self) -> None:
        async with self._start_lock:
            if self._context is not None:
                return
            try:
                from playwright.async_api import async_playwright
            except ImportError as e:  # pragma: no cover - the environment always has it
                raise BrowserFailure(f"Playwright isn't installed: {e}") from e
            self._pw = await async_playwright().start()
            try:
                if self.cfg.mode == "cdp":
                    await self._connect()
                else:
                    await self._launch()
            except BrowserFailure:
                await self._stop_playwright()
                raise
            except Exception as e:
                await self._stop_playwright()
                raise BrowserFailure(self._start_error(e)) from e

    def _start_error(self, e: BaseException) -> str:
        msg = first_line(e)
        if self.cfg.mode == "cdp":
            return (
                f"Couldn't connect to Chrome at {self.cfg.cdp_url}: {msg}. Start Chrome with "
                "--remote-debugging-port (see Settings → Browser), or switch Oxide to its own "
                "browser."
            )
        if "Executable doesn't exist" in msg or "playwright install" in msg:
            return (
                "The browser isn't installed yet. Open Settings → Browser and choose Install "
                "Chromium, or pick a browser installed on this computer."
            )
        return f"The browser didn't start: {msg}"

    async def _launch(self) -> None:
        port = free_port()
        self._profile = tempfile.mkdtemp(prefix="oxide-browser-")
        args = [
            f"--remote-debugging-port={port}",
            "--remote-debugging-address=127.0.0.1",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-features=Translate",
        ]
        kwargs: dict[str, Any] = {
            "headless": self.cfg.headless,
            "args": args,
            "viewport": {"width": 1280, "height": 800},
            "accept_downloads": False,
        }
        if self.cfg.executable:
            kwargs["executable_path"] = self.cfg.executable
        _log(f"starting the browser (headless={self.cfg.headless})")
        self._context = await self._pw.chromium.launch_persistent_context(self._profile, **kwargs)
        self.cdp_url = f"http://127.0.0.1:{port}"
        self._context.on("page", self._on_page)
        pages = list(self._context.pages)
        self._page = pages[0] if pages else await self._context.new_page()
        self._own_pages = [self._page]

    async def _connect(self) -> None:
        url = self.cfg.cdp_url or "http://127.0.0.1:9222"
        _log("connecting to a running Chrome")
        self._browser = await self._pw.chromium.connect_over_cdp(url, timeout=15_000)
        contexts = self._browser.contexts
        self._context = contexts[0] if contexts else await self._browser.new_context()
        self.cdp_url = url
        self._context.on("page", self._on_page)
        # Our own tab: the user's tabs are left alone.
        self._page = await self._context.new_page()
        self._own_pages = [self._page]

    def _on_page(self, page: Any) -> None:
        # Links that open a new tab: follow them there.
        self._page = page
        self._own_pages.append(page)

    async def _stop_playwright(self) -> None:
        if self._pw is not None:
            try:
                await self._pw.stop()
            except Exception:  # noqa: BLE001 - best effort on the way out
                pass
        self._pw = None

    async def close(self) -> str:
        """Closes the browser (or, with the user's Chrome, Oxide's own tabs)."""
        if self._context is None:
            return "The browser wasn't open."
        try:
            if self.cfg.mode == "cdp":
                for p in self._own_pages:
                    try:
                        if not p.is_closed():
                            await p.close()
                    except Exception:  # noqa: BLE001
                        pass
                if self._browser is not None:
                    # Disconnects; Chrome keeps running.
                    await self._browser.close()
            else:
                await self._context.close()
        except Exception as e:  # noqa: BLE001
            _log(f"closing: {first_line(e)}")
        self._context = None
        self._browser = None
        self._page = None
        self._own_pages = []
        await self._stop_playwright()
        if self._profile:
            shutil.rmtree(self._profile, ignore_errors=True)
            self._profile = None
        return "Closed the browser." if self.cfg.mode != "cdp" else "Closed Oxide's tabs."

    # ---- Pages ----------------------------------------------------------------------------

    async def page(self) -> Any:
        await self.start()
        p = self._page
        if p is None or p.is_closed():
            live = [x for x in self._context.pages if not x.is_closed()]
            if self.cfg.mode == "cdp":
                live = [x for x in live if x in self._own_pages]
            p = live[-1] if live else await self._context.new_page()
            if p not in self._own_pages:
                self._own_pages.append(p)
            self._page = p
        return p

    def current_page(self) -> Any:
        """The page in front, without starting anything (for screenshots during a task)."""
        if self._context is None:
            return None
        p = self._page
        if p is not None and not p.is_closed():
            return p
        live = [x for x in self._context.pages if not x.is_closed()]
        return live[-1] if live else None

    async def _settle(self, page: Any) -> None:
        try:
            await page.wait_for_load_state("domcontentloaded", timeout=ACTION_TIMEOUT_MS)
        except Exception:  # noqa: BLE001 - a page that keeps loading is still usable
            pass
        try:
            await page.wait_for_load_state("networkidle", timeout=SETTLE_MS)
        except Exception:  # noqa: BLE001
            pass

    async def page_text(self) -> dict[str, Any]:
        page = await self.page()
        try:
            return await page.evaluate(PAGE_TEXT_JS)
        except Exception as e:  # noqa: BLE001
            # Mid-navigation: wait and try once more.
            await self._settle(page)
            try:
                return await page.evaluate(PAGE_TEXT_JS)
            except Exception:  # noqa: BLE001
                raise BrowserFailure(f"The page couldn't be read: {first_line(e)}") from e

    async def _goto(self, page: Any, target: str) -> int | None:
        for attempt in range(3):
            try:
                resp = await page.goto(
                    target, wait_until="domcontentloaded", timeout=NAV_TIMEOUT_MS
                )
                break
            except Exception as e:  # noqa: BLE001
                # Right after a failed load Chrome is still showing its error page.
                if "interrupted by another navigation" not in str(e) or attempt == 2:
                    raise
                await asyncio.sleep(0.5)
        await self._settle(page)
        return resp.status if resp is not None else None

    async def navigate(self, url: str) -> dict[str, Any]:
        """Opens an address. Search words are searched; an address that doesn't exist (unknown
        site, 404 or 410) is searched for instead, so the model gets real links rather than
        guessing another address. Other errors (403, 500, …) come with a note."""
        if is_search_words(url):
            info = await self.search(url.strip())
            info["note"] = f'Searched for "{url.strip()}". Open a result by its number with click.'
            return info
        target = normalize_url(url)
        page = await self.page()
        try:
            status = await self._goto(page, target)
        except Exception as e:  # noqa: BLE001
            msg = first_line(e)
            # An error answer without a page (Chrome shows nothing): searched for, like a 404.
            failed = "ERR_HTTP_RESPONSE_CODE_FAILURE" in msg
            if not is_unknown_host(msg) and not failed:
                raise BrowserFailure(f"Couldn't open {target}: {msg}") from e
            words = query_from_url(target)
            info = await self._search_instead(target, words)
            why = "answered with an error" if failed else "doesn't exist (the site's name isn't known)"
            info["note"] = (
                f"{target} {why}. Searched for "
                f'"{words}" instead: open a real result below by its number with click, rather '
                "than guessing another address."
            )
            return info
        if status in GONE_STATUSES:
            words = query_from_url(target)
            info = await self._search_instead(target, words)
            info["note"] = (
                f"{target} answered {status} (not found). Searched for \"{words}\" instead: open "
                "a real result below by its number with click, rather than guessing another "
                "address."
            )
            return info
        info = await self.page_text()
        info["status"] = status
        if status is not None and status >= 400:
            why = " (access denied)" if status in (401, 403) else ""
            info["note"] = (
                f"{target} answered {status}{why}. If this isn't the page you need, search for "
                f"the right one: call navigate with search words as the url (they are "
                f"searched on Google, e.g. {search_url(query_from_url(target), self.cfg.search_url)}) "
                "and open a real result, rather than guessing another address."
            )
        return info

    async def _search_instead(self, target: str, words: str) -> dict[str, Any]:
        try:
            return await self.search(words)
        except BrowserFailure as e:
            raise BrowserFailure(f"{target} doesn't work, and searching for it failed: {e}") from e

    def _engine(self) -> str:
        return self.cfg.search_url or GOOGLE_SEARCH

    async def _search_page(self) -> Any:
        """The tab searches run in. With the user's Chrome, a tab they keep on the search
        engine (e.g. google.com) is used when there is one; otherwise the current page."""
        page = await self.page()
        if self.cfg.mode != "cdp":
            return page
        engine = host_of(search_url("x", self._engine()))
        for p in self._context.pages:
            if p.is_closed() or p in self._own_pages:
                continue
            if host_of(p.url) == engine:
                self._page = p
                return p
        return page

    def _blocked_message(self) -> str:
        if self.cfg.mode == "cdp" or not self.cfg.headless:
            return (
                "The search was refused: the search engine is showing a check for robots in "
                "the browser window. Don't try to solve or get around it. Ask the user to "
                "complete it in their browser window, then search again."
            )
        return (
            "The search was refused: the search engine showed Oxide's hidden browser a check "
            "for robots. Don't try to get around it. Use web_search if you have it; otherwise "
            "tell the user, who can switch Settings → Browser to “My own Chrome” (their own, "
            "visible Chrome) for searching."
        )

    async def search(self, words: str) -> dict[str, Any]:
        """Searches the web in the browser (Google, its cookie page answered) and returns the
        results page. A check for robots is reported as a refused search."""
        page = await self._search_page()
        try:
            await self._goto(page, search_url(words, self._engine()))
        except Exception as e:  # noqa: BLE001
            raise BrowserFailure(f"The search didn't work: {first_line(e)}") from e
        if "consent.google." in page.url:
            await self._answer_consent(page)
        info = await self.page_text()
        if is_blocked(page.url, info.get("text") or ""):
            _log(f"the search engine showed a check for robots ({host_of(page.url)})")
            raise BrowserFailure(self._blocked_message())
        info["status"] = None
        return info

    async def _answer_consent(self, page: Any) -> None:
        for pattern in (CONSENT_REJECT, CONSENT_ACCEPT):
            try:
                button = page.get_by_role("button", name=pattern)
                if await button.count() > 0:
                    await button.first.click(timeout=ACTION_TIMEOUT_MS)
                    await asyncio.sleep(0.5)
                    await self._settle(page)
                    return
            except Exception:  # noqa: BLE001 - the fallback search follows
                pass

    def _locator(self, page: Any, ref: int | None, text: str | None) -> Any:
        if ref is not None:
            return page.locator(f'[{REF_ATTR}="{int(ref)}"]').first
        if text:
            # Visible matches only: hidden copies of the text (menus, templates) can't be clicked.
            return page.get_by_text(text, exact=False).filter(visible=True).first
        raise BrowserFailure("Say which element: give `ref` (a number from read_page) or `text`.")

    async def _check_exists(self, loc: Any, ref: int | None, text: str | None) -> None:
        try:
            count = await loc.count()
        except Exception:  # noqa: BLE001
            count = 0
        if count == 0:
            if ref is not None:
                raise BrowserFailure(
                    f"There is no element [{ref}] on the page now. Call read_page again: "
                    "numbers change when the page does."
                )
            raise BrowserFailure(f'No element with the text "{text}" is on the page.')

    async def _front(self, page: Any) -> None:
        """Brings the page's tab to the front in a visible browser. Chrome hardly renders a
        tab in the background, and clicks there wait for it until they time out."""
        if self.cfg.mode == "cdp" or not self.cfg.headless:
            try:
                await page.bring_to_front()
            except Exception:  # noqa: BLE001 - best effort
                pass

    async def click(self, ref: int | None, text: str | None) -> dict[str, Any]:
        page = await self.page()
        await self._front(page)
        loc = self._locator(page, ref, text)
        await self._check_exists(loc, ref, text)
        try:
            await loc.click(timeout=ACTION_TIMEOUT_MS)
        except Exception as e:  # noqa: BLE001
            problem = click_problem(str(e))
            if problem is not None:
                raise BrowserFailure(f"The click didn't work: {problem}") from e
            # It timed out waiting for the element to hold still (an animation, or a page
            # Chrome isn't rendering): click it directly instead.
            try:
                await loc.evaluate("el => el.click()", timeout=5_000)
            except Exception as e2:  # noqa: BLE001
                raise BrowserFailure(f"The click didn't work: {first_line(e2)}") from e2
        await asyncio.sleep(0.3)
        page = await self.page()
        await self._settle(page)
        return await self.page_text()

    async def type(
        self, ref: int | None, text: str, submit: bool, clear: bool, field_text: str | None
    ) -> dict[str, Any]:
        page = await self.page()
        await self._front(page)
        loc = self._locator(page, ref, field_text)
        await self._check_exists(loc, ref, field_text)
        try:
            if clear:
                await loc.fill(text, timeout=ACTION_TIMEOUT_MS)
            else:
                await loc.press_sequentially(text, timeout=ACTION_TIMEOUT_MS)
            if submit:
                await loc.press("Enter", timeout=ACTION_TIMEOUT_MS)
        except Exception as e:  # noqa: BLE001
            raise BrowserFailure(f"Typing didn't work: {first_line(e)}") from e
        if submit:
            await asyncio.sleep(0.3)
            page = await self.page()
            await self._settle(page)
        return await self.page_text()

    async def screenshot(self, full_page: bool = False, quality: int = 70) -> bytes:
        page = await self.page()
        try:
            return await page.screenshot(
                type="jpeg", quality=quality, full_page=full_page, timeout=15_000
            )
        except Exception as e:  # noqa: BLE001
            raise BrowserFailure(f"The screenshot failed: {first_line(e)}") from e

    async def frame(self) -> tuple[bytes, str, str] | None:
        """A small screenshot of the page in front, its URL and title; None when there is
        none (or it is busy)."""
        page = self.current_page()
        if page is None:
            return None
        try:
            data = await page.screenshot(type="jpeg", quality=45, timeout=5_000)
            title = await asyncio.wait_for(page.title(), 2)
            return data, page.url, title
        except Exception:  # noqa: BLE001 - frames are best effort
            return None
