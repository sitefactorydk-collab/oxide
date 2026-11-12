"""Drives a real Chrome with Oxide's extension for crates/oxide-bridge/tests/real_chrome.rs.

Usage: drive_chrome.py <chrome> <extension dir> <port> <code> <origin to allow>

Pairs through the extension's own popup (typing the code like a user), checks that asking
per site is off, prints READY, then answers commands on stdin, one per line:
  state            the popup's link state (connected, offline, unpaired, …)
  banner           how many open pages show Oxide's banner
  stop             presses "Stop Oxide" in the popup
  block <origin>   blocks a site in the popup
  quit
Needs Python Playwright (no browser download: it drives the Chrome given).
"""

from __future__ import annotations

import asyncio
import sys
import tempfile

from playwright.async_api import async_playwright


async def main() -> None:
    chrome, ext, port, code, allow = sys.argv[1:6]
    async with async_playwright() as p:
        ctx = await p.chromium.launch_persistent_context(
            tempfile.mkdtemp(prefix="oxide-ext-"),
            executable_path=chrome,
            headless=True,
            ignore_default_args=["--disable-extensions"],
            args=[
                "--headless=new",
                f"--disable-extensions-except={ext}",
                f"--load-extension={ext}",
                "--disable-features=DisableLoadExtensionCommandLineSwitch",
                "--no-first-run",
                "--no-sandbox",
            ],
        )
        for _ in range(100):
            if ctx.service_workers:
                break
            await asyncio.sleep(0.1)
        ext_id = ctx.service_workers[0].url.split("/")[2]
        popup = await ctx.new_page()
        await popup.goto(f"chrome-extension://{ext_id}/popup.html")
        await popup.fill("#code", code)
        await popup.fill("#port", port)
        await popup.click("#pair-button")
        await popup.wait_for_selector('#state[data-state="connected"]', timeout=15_000)
        # Asking per site is off by default: the test site is used without allowing it first
        # (a prompt would hang the first navigate). `allow` stays an argument for older callers.
        assert not await popup.is_checked("#ask-sites"), "asking per site should be off"
        del allow
        print("READY", ext_id, flush=True)

        loop = asyncio.get_running_loop()
        while True:
            line = (await loop.run_in_executor(None, sys.stdin.readline)).strip()
            if not line or line == "quit":
                break
            cmd, _, arg = line.partition(" ")
            if cmd == "state":
                await popup.wait_for_timeout(2_000)  # the popup refreshes every 1.5 s
                print(await popup.get_attribute("#state", "data-state"), flush=True)
            elif cmd == "banner":
                n = 0
                for pg in ctx.pages:
                    if pg.url.startswith("http"):
                        try:
                            n += await pg.evaluate("document.querySelectorAll('#oxide-banner').length")
                        except Exception:
                            pass
                print(n, flush=True)
            elif cmd == "stop":
                await popup.wait_for_selector("#stop-all", state="visible", timeout=5_000)
                await popup.click("#stop-all")
                print("STOPPED", flush=True)
            elif cmd == "block":
                await popup.fill("#site", arg)
                await popup.select_option("#site-choice", "block")
                await popup.click("#site-form button")
                await popup.wait_for_selector("#sites .tag.block", timeout=5_000)
                print("BLOCKED", flush=True)
            else:
                print("?", flush=True)
        await ctx.close()


if __name__ == "__main__":
    asyncio.run(main())
