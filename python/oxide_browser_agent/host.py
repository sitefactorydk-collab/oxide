"""The browser: started (or reached over CDP) on first use and shared by every tool.

Oxide's own Chromium runs with a throwaway profile and a remote-debugging port on
127.0.0.1, so Browser-Use (``agent.py``) can drive the same browser the granular tools use.
In "cdp" mode the user's running Chrome is used instead: Oxide opens its own tab there and
never closes the user's tabs or the browser.
"""

from __future__ import annotations

import asyncio
import shutil
import socket
import sys
import tempfile
from typing import Any

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

    async def navigate(self, url: str) -> dict[str, Any]:
        target = normalize_url(url)
        page = await self.page()
        status = None
        try:
            resp = await page.goto(target, wait_until="domcontentloaded", timeout=NAV_TIMEOUT_MS)
            status = resp.status if resp is not None else None
        except Exception as e:  # noqa: BLE001
            raise BrowserFailure(f"Couldn't open {target}: {first_line(e)}") from e
        await self._settle(page)
        info = await self.page_text()
        info["status"] = status
        return info

    def _locator(self, page: Any, ref: int | None, text: str | None) -> Any:
        if ref is not None:
            return page.locator(f'[{REF_ATTR}="{int(ref)}"]').first
        if text:
            return page.get_by_text(text, exact=False).first
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

    async def click(self, ref: int | None, text: str | None) -> dict[str, Any]:
        page = await self.page()
        loc = self._locator(page, ref, text)
        await self._check_exists(loc, ref, text)
        try:
            await loc.click(timeout=ACTION_TIMEOUT_MS)
        except Exception as e:  # noqa: BLE001
            raise BrowserFailure(f"The click didn't work: {first_line(e)}") from e
        await asyncio.sleep(0.3)
        page = await self.page()
        await self._settle(page)
        return await self.page_text()

    async def type(
        self, ref: int | None, text: str, submit: bool, clear: bool, field_text: str | None
    ) -> dict[str, Any]:
        page = await self.page()
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
