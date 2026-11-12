"""Tests of the browser agent against a real Chromium and a local web server.

Needs Playwright and a browser: set OXIDE_TEST_CHROME to a Chrome/Chromium executable (or
have Playwright's own Chromium installed). Browser-Use is replaced by a stand-in that drives
the same browser over CDP, so no model is needed.

    OXIDE_TEST_CHROME=/usr/bin/google-chrome python -m unittest discover -s python/tests
"""

from __future__ import annotations

import asyncio
import base64
import json
import os
import sys
import threading
import types
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from oxide_browser_agent import agent as agent_mod  # noqa: E402
from oxide_browser_agent.config import Config  # noqa: E402
from oxide_browser_agent.host import (  # noqa: E402
    BrowserFailure,
    BrowserHost,
    click_problem,
    free_port,
    is_blocked,
    is_search_words,
    is_unknown_host,
    normalize_url,
    query_from_url,
    search_url,
)
from oxide_browser_agent.pagetext import slice_text  # noqa: E402

PAGES = {
    "/": """<!doctype html><title>Bakery</title>
<h1>The Little Bakery</h1>
<p>Fresh bread every day.</p>
<nav><a href="/hours">Opening hours</a> <a href="/about" style="display:none">Hidden</a></nav>
<form action="/search"><label for="q">Search</label><input id="q" name="q" placeholder="Find bread">
<button type="submit">Go</button></form>
<input type="checkbox" id="c" checked aria-label="Newsletter">
<script>document.write('')</script>""",
    "/tricky": """<!doctype html><title>Tricky</title>
<style>@keyframes wiggle { from { transform: translateX(0) } to { transform: translateX(40px) } }
.wiggle { display: inline-block; animation: wiggle 0.2s infinite alternate; }
#cover { position: fixed; inset: 0; background: rgba(0,0,0,.3); }</style>
<a class="wiggle" href="/hours">Moving link</a>
<button id="b" onclick="document.title='clicked'">Covered button</button>
<div id="cover"></div>""",
    "/hours": """<!doctype html><title>Hours</title><h2>Opening hours</h2>
<ul><li>Mon–Fri: 7–18</li><li>Sat: 8–14</li><li>Sun: closed</li></ul>""",
}


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):  # noqa: N802
        u = urlparse(self.path)
        if u.path == "/search":
            q = parse_qs(u.query).get("q", [""])[0]
            if q == "tivoli hours":
                body = """<!doctype html><title>tivoli hours - Search</title>
<div id="top"><div><div><span>AI Overview</span></div>
<div><p>Tivoli Gardens in Copenhagen is open daily from 11:00 to 22:00 in the summer season,
with longer hours on Fridays.</p><p id="more" hidden>Tickets for adults cost 185 DKK.</p>
<button onclick="document.getElementById('more').hidden=false">Show more</button></div></div></div>
<div><a href="/hours">Opening hours - Tivoli</a><p>Plan your visit.</p></div>
<div><a href="/">Tivoli Gardens - Wikipedia</a><p>An amusement park.</p></div>"""
            elif q == "robot":
                body = "<title>Sorry</title><p>Our systems have detected unusual traffic.</p>"
            else:
                body = f"<!doctype html><title>Results</title><p>You searched for {q}</p>"
        elif u.path in PAGES:
            body = PAGES[u.path]
        elif u.path == "/empty-404":
            self.send_response(404)
            self.end_headers()
            return
        else:
            data = b"<!doctype html><title>Not found</title><h1>Page not found</h1>"
            self.send_response(404)
            self.send_header("Content-Type", "text/html")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        data = body.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *args):  # quiet
        pass


def chrome() -> str:
    return os.environ.get("OXIDE_TEST_CHROME", "")


def have_playwright() -> bool:
    try:
        import playwright  # noqa: F401
    except ImportError:
        return False
    return True


class Web:
    def __enter__(self):
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        return self

    def __exit__(self, *a):
        self.server.shutdown()


def cfg(**kw) -> Config:
    c = Config(executable=chrome(), headless=True)
    for k, v in kw.items():
        setattr(c, k, v)
    return c


class PureTests(unittest.TestCase):
    def test_urls_get_a_scheme_and_bad_ones_are_refused(self):
        self.assertEqual(normalize_url("example.com"), "https://example.com")
        self.assertEqual(normalize_url("localhost:8080/x"), "http://localhost:8080/x")
        self.assertEqual(normalize_url("http://a.b"), "http://a.b")
        with self.assertRaises(BrowserFailure):
            normalize_url("javascript:alert(1)")
        with self.assertRaises(BrowserFailure):
            normalize_url("  ")

    def test_search_words_and_queries_for_dead_addresses(self):
        self.assertTrue(is_search_words("opening hours national museum"))
        self.assertTrue(is_search_words("weather"))
        self.assertFalse(is_search_words("natmus.dk/opening-hours"))
        self.assertFalse(is_search_words("localhost:3000"))
        self.assertFalse(is_search_words("https://example.com/a b"))
        self.assertEqual(
            query_from_url("https://www.natmus.dk/museer/nationalmuseet/aabningstider.html?x=1"),
            "natmus.dk museer nationalmuseet aabningstider",
        )
        self.assertEqual(
            search_url("hours & prices"), "https://www.google.com/search?hl=en&q=hours+%26+prices"
        )
        self.assertEqual(search_url("a b", "http://s/?q={q}"), "http://s/?q=a+b")
        self.assertTrue(is_unknown_host("net::ERR_NAME_NOT_RESOLVED at https://nope.invalid/"))
        self.assertTrue(is_blocked("https://www.google.com/sorry/index?x", ""))
        self.assertIn("covers it", click_problem("<div id=x> intercepts pointer events") or "")
        self.assertIsNone(click_problem("waiting for element to be visible, enabled and stable"))
        self.assertTrue(is_blocked("https://x", "Our systems have detected unusual traffic"))
        self.assertFalse(is_blocked("https://www.google.com/search?q=a", "Results for a"))
        self.assertFalse(is_unknown_host("net::ERR_CONNECTION_REFUSED"))

    def test_the_ai_overview_leads_the_results_text(self):
        from oxide_browser_agent.server import page_summary

        text = page_summary(
            {"title": "t", "url": "u", "text": "RESULTS-BODY", "overview": "Open 11-22."}, 1000
        )
        self.assertIn("Google's AI Overview", text)
        self.assertLess(text.index("Open 11-22."), text.index("RESULTS-BODY"))
        self.assertNotIn("AI Overview", page_summary({"title": "t", "url": "u", "text": "r"}, 1000))

    def test_slices_say_where_they_are(self):
        self.assertEqual(slice_text("abc", 0, 10), "abc")
        s = slice_text("a" * 50, 10, 20)
        self.assertIn("Starts at character 10 of 50", s)
        self.assertIn("start=30", s)

    def test_progress_messages_are_json_with_a_frame(self):
        m = json.loads(agent_mod.progress_message("Browsing", b"\xff\xd8x", "http://a"))
        self.assertEqual(m["text"], "Browsing")
        self.assertEqual(base64.b64decode(m["frame"]), b"\xff\xd8x")
        self.assertEqual(m["url"], "http://a")
        self.assertNotIn("frame", json.loads(agent_mod.progress_message("x")))

    def test_summaries_cover_done_unfinished_and_failed(self):
        class H:
            def __init__(self, final, done, ok, errors=()):
                self._f, self._d, self._o, self._e = final, done, ok, list(errors)

            def final_result(self):
                return self._f

            def is_done(self):
                return self._d

            def is_successful(self):
                return self._o

            def errors(self):
                return self._e

        text, failed = agent_mod.summarize(H("Open 7-18", True, True), 5)
        self.assertIn("finished the task", text)
        self.assertIn("Open 7-18", text)
        self.assertFalse(failed)
        text, failed = agent_mod.summarize(H(None, False, None, ["boom\nmore"]), 5)
        self.assertIn("stopped before finishing (limit: 5 steps)", text)
        self.assertIn("- boom", text)
        self.assertTrue(failed)
        text, failed = agent_mod.summarize(H("partial", True, False), 5)
        self.assertIn("didn't manage", text)
        self.assertFalse(failed)


def fake_browser_use(record: dict) -> types.ModuleType:
    """A stand-in for Browser-Use: its agent opens /hours over CDP, like the real one would."""
    m = types.ModuleType("browser_use")

    class ChatOpenAI:
        def __init__(self, model, api_key=None, base_url=None):
            record["llm"] = ("openai", model, api_key, base_url)

    class ChatAnthropic:
        def __init__(self, model, api_key=None):
            record["llm"] = ("anthropic", model, api_key, os.environ.get("ANTHROPIC_BASE_URL"))

    class Browser:
        def __init__(self, cdp_url=None, keep_alive=None):
            record["cdp_url"] = cdp_url

    class History:
        def __init__(self, final):
            self.final = final

        def final_result(self):
            return self.final

        def is_done(self):
            return True

        def is_successful(self):
            return True

        def errors(self):
            return []

    class Agent:
        def __init__(self, task, llm=None, browser=None, use_vision=True):
            record["task"] = task
            record["use_vision"] = use_vision
            self.browser = browser

        async def run(self, max_steps=100):
            record["max_steps"] = max_steps
            from playwright.async_api import async_playwright

            async with async_playwright() as p:
                b = await p.chromium.connect_over_cdp(record["cdp_url"])
                page = b.contexts[0].pages[0]
                await page.goto(record["target"])
                await asyncio.sleep(3.5)  # long enough for a frame or two
                text = await page.inner_text("body")
            return History(text.strip().splitlines()[1])

    m.ChatOpenAI = ChatOpenAI
    m.ChatAnthropic = ChatAnthropic
    m.Browser = Browser
    m.Agent = Agent
    return m


@unittest.skipUnless(have_playwright(), "needs Playwright")
class BrowserTests(unittest.IsolatedAsyncioTestCase):
    async def test_navigate_read_click_type_screenshot_and_close(self):
        with Web() as web:
            host = BrowserHost(cfg())
            try:
                info = await host.navigate(web.url)
                text = info["text"]
                self.assertEqual(info["title"], "Bakery")
                self.assertIn("# The Little Bakery", text)
                self.assertIn('[1] link "Opening hours"', text)
                self.assertNotIn("Hidden", text)
                self.assertIn('textbox "Search"', text)
                self.assertIn('checkbox "Newsletter" (checked)', text)
                self.assertIn('button "Go"', text)

                info = await host.click(1, None)
                self.assertTrue(info["url"].endswith("/hours"), info["url"])
                self.assertIn("Sun: closed", info["text"])
                self.assertIn("- Mon–Fri: 7–18", info["text"])

                with self.assertRaises(BrowserFailure) as e:
                    await host.click(99, None)
                self.assertIn("read_page again", str(e.exception))

                await host.navigate(web.url)
                info = await host.page_text()
                ref = next(
                    int(line.split("]")[0].split("[")[-1])
                    for line in info["text"].splitlines()
                    if 'textbox "Search"' in line
                )
                info = await host.type(ref, "rye", True, True, None)
                self.assertIn("You searched for rye", info["text"])

                # By visible text, too.
                await host.navigate(web.url)
                info = await host.click(None, "Opening hours")
                self.assertTrue(info["url"].endswith("/hours"))

                shot = await host.screenshot()
                self.assertEqual(shot[:2], b"\xff\xd8")
                frame = await host.frame()
                self.assertIsNotNone(frame)
                self.assertTrue(frame[1].endswith("/hours"))
                self.assertEqual(await host.close(), "Closed the browser.")
                self.assertFalse(host.started)
                self.assertEqual(await host.close(), "The browser wasn't open.")
            finally:
                if host.started:
                    await host.close()

    async def test_search_and_dead_addresses_lead_to_real_results(self):
        with Web() as web:
            host = BrowserHost(cfg(search_url=web.url + "/search?q={q}"))
            try:
                info = await host.search("opening hours bakery")
                self.assertIn("You searched for opening hours bakery", info["text"])
                # Search words given to navigate are searched.
                info = await host.navigate("bakery opening hours")
                self.assertIn("You searched for bakery opening hours", info["text"])
                self.assertIn("Searched for", info["note"])
                # A page that isn't there: searched for instead, with a note.
                info = await host.navigate(web.url + "/opening-hours.html")
                self.assertIn("answered 404", info["note"])
                self.assertIn("You searched for 127.0.0.1 opening hours", info["text"])
                # An error answer without a page, too.
                info = await host.navigate(web.url + "/empty-404")
                self.assertIn("answered with an error", info["note"])
                self.assertIn("You searched for 127.0.0.1 empty", info["text"])
                # Google's AI Overview comes first, opened up.
                info = await host.search("tivoli hours")
                self.assertIn("open daily from 11:00 to 22:00", info["overview"])
                self.assertIn("185 DKK", info["overview"])
                self.assertNotIn("Wikipedia", info["overview"])
                info = await host.search("opening hours bakery")
                self.assertNotIn("overview", info)
                # A check for robots is a refused search, never something to get around.
                with self.assertRaises(BrowserFailure) as e:
                    await host.search("robot")
                self.assertIn("refused", str(e.exception))
                self.assertIn("Don't try to get around it", str(e.exception))
                # A real page has no note.
                info = await host.navigate(web.url + "/hours")
                self.assertNotIn("note", info)
            finally:
                await host.close()

    async def test_with_the_users_chrome_a_tab_on_the_search_engine_is_used_and_kept(self):
        from playwright.async_api import async_playwright

        with Web() as web:
            port = free_port()
            async with async_playwright() as p:
                kw = {"headless": True, "args": [f"--remote-debugging-port={port}"]}
                if chrome():
                    kw["executable_path"] = chrome()
                users = await p.chromium.launch(**kw)
                try:
                    tab = await (await users.new_context()).new_page()
                    await tab.goto(web.url + "/")
                    host = BrowserHost(
                        cfg(
                            mode="cdp",
                            cdp_url=f"http://127.0.0.1:{port}",
                            search_url=web.url + "/search?q={q}",
                        )
                    )
                    info = await host.search("opening hours")
                    self.assertIn("You searched for opening hours", info["text"])
                    # The search ran in the user's tab on the search engine's site.
                    self.assertIn("/search?q=opening", tab.url)
                    with self.assertRaises(BrowserFailure) as e:
                        await host.search("robot")
                    self.assertIn("Ask the user to complete it", str(e.exception))
                    await host.close()
                    self.assertFalse(tab.is_closed())
                finally:
                    await users.close()

    async def test_moving_elements_are_clicked_and_covered_ones_explained(self):
        from oxide_browser_agent import host as host_mod

        saved = host_mod.ACTION_TIMEOUT_MS
        host_mod.ACTION_TIMEOUT_MS = 1500
        with Web() as web:
            host = BrowserHost(cfg())
            try:
                info = await host.navigate(web.url + "/tricky")
                # The covering layer is above everything: the button can't be clicked.
                import re

                ref = int(re.search(r'\[(\d+)\] button "Covered button"', info["text"]).group(1))
                with self.assertRaises(BrowserFailure) as e:
                    await host.click(ref, None)
                self.assertIn("covers it", str(e.exception))
                # A link that never holds still is clicked directly after the wait.
                await host.navigate(web.url + "/tricky")
                await host.page()
                page = await host.page()
                await page.evaluate("document.getElementById('cover').remove()")
                info = await host.click(None, "Moving link")
                self.assertTrue(info["url"].endswith("/hours"), info["url"])
            finally:
                host_mod.ACTION_TIMEOUT_MS = saved
                await host.close()

    async def test_a_missing_browser_says_how_to_get_one(self):
        host = BrowserHost(cfg(executable="/nope/chrome"))
        with self.assertRaises(BrowserFailure) as e:
            await host.navigate("http://127.0.0.1:9/")
        msg = str(e.exception)
        self.assertTrue("didn't start" in msg or "isn't installed" in msg, msg)
        self.assertFalse(host.started)

    async def test_cdp_mode_without_chrome_explains(self):
        host = BrowserHost(cfg(mode="cdp", cdp_url="http://127.0.0.1:9"))
        with self.assertRaises(BrowserFailure) as e:
            await host.start()
        self.assertIn("--remote-debugging-port", str(e.exception))

    async def test_browser_task_runs_browser_use_in_the_same_browser_with_frames(self):
        record: dict = {}
        sys.modules["browser_use"] = fake_browser_use(record)
        try:
            with Web() as web:
                record["target"] = web.url + "/hours"
                host = BrowserHost(cfg())
                c = cfg(llm_kind="lmstudio", llm_model="qwen", llm_base_url="http://127.0.0.1:1234/v1")
                reports: list[tuple[float, dict]] = []

                async def report(n, msg):
                    reports.append((n, json.loads(msg)))

                try:
                    await host.navigate(web.url)
                    result = await agent_mod.run_task(host, c, "Find the opening hours", 7, report)
                finally:
                    await host.close()
            self.assertEqual(record["llm"], ("openai", "qwen", "not-needed", "http://127.0.0.1:1234/v1"))
            self.assertEqual(record["task"], "Find the opening hours")
            self.assertEqual(record["max_steps"], 7)
            self.assertIs(record["use_vision"], False)
            self.assertTrue(record["cdp_url"].startswith("http://127.0.0.1:"))
            self.assertFalse(result.is_error, result.text)
            self.assertIn("finished the task", result.text)
            self.assertIn("Mon–Fri: 7–18", result.text)
            self.assertIn("/hours", result.text)
            self.assertTrue(result.shots and result.shots[-1][:2] == b"\xff\xd8")
            self.assertTrue(reports, "no live frames")
            self.assertIn("frame", reports[-1][1])
            self.assertTrue(reports[-1][1]["text"].startswith("Browsing: "))
        finally:
            sys.modules.pop("browser_use", None)

    async def test_browser_task_needs_a_model_and_anthropic_gets_its_base_url(self):
        record: dict = {}
        sys.modules["browser_use"] = fake_browser_use(record)
        try:
            with self.assertRaises(BrowserFailure) as e:
                agent_mod.make_llm(cfg())
            self.assertIn("no model", str(e.exception))
            os.environ.pop("ANTHROPIC_BASE_URL", None)
            agent_mod.make_llm(
                cfg(llm_kind="anthropic", llm_model="claude", llm_api_key="k", llm_base_url="https://api.anthropic.com/v1")
            )
            self.assertEqual(record["llm"], ("anthropic", "claude", "k", None))
            agent_mod.make_llm(
                cfg(llm_kind="anthropic", llm_model="claude", llm_api_key="k", llm_base_url="https://proxy.example/v1")
            )
            self.assertEqual(record["llm"][3], "https://proxy.example")
        finally:
            os.environ.pop("ANTHROPIC_BASE_URL", None)
            sys.modules.pop("browser_use", None)

    async def test_without_browser_use_the_task_points_to_the_granular_tools(self):
        sys.modules["browser_use"] = None  # type: ignore[assignment]  # import fails
        try:
            with self.assertRaises(BrowserFailure) as e:
                agent_mod.make_llm(cfg(llm_kind="openai", llm_model="gpt"))
            self.assertIn("navigate, read_page, click", str(e.exception))
        finally:
            sys.modules.pop("browser_use", None)


if __name__ == "__main__":
    unittest.main()
