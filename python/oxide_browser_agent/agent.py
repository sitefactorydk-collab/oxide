"""``browser_task``: Browser-Use works through a goal on its own, in the shared browser.

Browser-Use's model calls go to Oxide's selected provider (``OXIDE_LLM_*``): Anthropic
through ``ChatAnthropic``, everything else (OpenAI, LM Studio, Strata) through
``ChatOpenAI`` with the endpoint's base URL. While it works, a screenshot of the page in
front is sent as progress every couple of seconds, so Oxide can show a live view.
"""

from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import os
import sys
from dataclasses import dataclass, field
from typing import Any, Awaitable, Callable

from .config import Config
from .host import BrowserFailure, BrowserHost, first_line

FRAME_EVERY = 1.5
# Screenshots kept for the result (the last ones of different pages).
KEEP_SHOTS = 3

Report = Callable[[float, str], Awaitable[None]]


@dataclass
class TaskResult:
    text: str
    shots: list[bytes] = field(default_factory=list)
    is_error: bool = False


def _log(msg: str) -> None:
    print(f"[oxide-browser] {msg}", file=sys.stderr, flush=True)


def progress_message(text: str, frame: bytes | None = None, url: str | None = None) -> str:
    """What a progress notice carries: JSON Oxide reads (text, URL and a JPEG frame)."""
    msg: dict[str, Any] = {"text": text}
    if url:
        msg["url"] = url
    if frame:
        msg["frame"] = base64.b64encode(frame).decode("ascii")
    return json.dumps(msg)


def make_llm(cfg: Config) -> Any:
    """Browser-Use's chat model for Oxide's provider."""
    if not cfg.llm_model:
        raise BrowserFailure(
            "browser_task has no model to run with. Pick a model in the chat (or in Settings → "
            "Browser), or use navigate, read_page, click and type yourself."
        )
    try:
        import browser_use  # noqa: F401
    except ImportError as e:
        raise BrowserFailure(
            f"Browser-Use isn't available ({first_line(e)}). Use navigate, read_page, click "
            "and type instead."
        ) from e
    if cfg.llm_kind == "anthropic":
        from browser_use import ChatAnthropic

        base = cfg.llm_base_url.rstrip("/")
        if base.endswith("/v1"):
            base = base[:-3]
        if base and base != "https://api.anthropic.com":
            os.environ["ANTHROPIC_BASE_URL"] = base
        return ChatAnthropic(cfg.llm_model, api_key=cfg.llm_api_key or None)
    from browser_use import ChatOpenAI

    # Local servers need no key, but the OpenAI client wants one.
    return ChatOpenAI(
        cfg.llm_model,
        api_key=cfg.llm_api_key or "not-needed",
        base_url=cfg.llm_base_url or None,
    )


def _make_browser(cdp_url: str) -> Any:
    from browser_use import Browser

    try:
        return Browser(cdp_url=cdp_url, keep_alive=True)
    except TypeError:
        return Browser(cdp_url=cdp_url)


def _make_agent(goal: str, llm: Any, browser: Any, vision: bool) -> Any:
    from browser_use import Agent

    kwargs: dict[str, Any] = {"llm": llm, "browser": browser}
    if not vision:
        # Honoured by Browser-Use: no screenshots go to a model that can't see them.
        kwargs["use_vision"] = False
    try:
        return Agent(goal, **kwargs)
    except TypeError:
        kwargs.pop("use_vision", None)
        return Agent(goal, **kwargs)


def _call(obj: Any, name: str) -> Any:
    f = getattr(obj, name, None)
    if not callable(f):
        return None
    try:
        return f()
    except Exception:  # noqa: BLE001 - result helpers differ between versions
        return None


def summarize(history: Any, steps_limit: int) -> tuple[str, bool]:
    """The model-facing text of a finished run, and whether it failed."""
    final = _call(history, "final_result")
    done = _call(history, "is_done")
    ok = _call(history, "is_successful")
    errors = _call(history, "errors") or []
    errors = [first_line(Exception(str(e))) for e in errors if e]
    if done is False or done is None and not final:
        head = f"Browser-Use stopped before finishing (limit: {steps_limit} steps)."
        failed = True
    elif ok is False:
        head = "Browser-Use finished but says it didn't manage the task."
        failed = False
    else:
        head = "Browser-Use finished the task."
        failed = False
    parts = [head]
    if final:
        parts.append(f"Result:\n{final}")
    if errors:
        parts.append("Errors on the way:\n" + "\n".join(f"- {e}" for e in errors[-5:]))
    return "\n\n".join(parts), failed and not final


async def run_task(
    host: BrowserHost, cfg: Config, goal: str, max_steps: int | None, report: Report
) -> TaskResult:
    goal = (goal or "").strip()
    if not goal:
        raise BrowserFailure("browser_task needs a goal.")
    steps = max(1, min(200, max_steps or cfg.max_steps))
    llm = make_llm(cfg)
    await host.start()
    if not host.cdp_url:
        raise BrowserFailure("The browser has no debugging address for Browser-Use.")
    browser = _make_browser(host.cdp_url)
    agent = _make_agent(goal, llm, browser, cfg.llm_vision)

    shots: list[tuple[str, bytes]] = []
    seen = {"hash": "", "n": 0}

    async def frames() -> None:
        while True:
            await asyncio.sleep(FRAME_EVERY)
            got = await host.frame()
            if got is None:
                continue
            data, url, title = got
            digest = hashlib.sha1(data).hexdigest()
            if digest == seen["hash"]:
                continue
            seen["hash"] = digest
            seen["n"] += 1
            if shots and shots[-1][0] == url:
                shots[-1] = (url, data)
            else:
                shots.append((url, data))
                del shots[:-KEEP_SHOTS]
            label = title.strip() or url
            try:
                await report(seen["n"], progress_message(f"Browsing: {label}", data, url))
            except Exception:  # noqa: BLE001 - the client may not want progress
                pass

    _log(f"browser_task with {cfg.describe_llm()}, up to {steps} steps")
    ticker = asyncio.create_task(frames())
    try:
        try:
            run = agent.run(max_steps=steps)
        except TypeError:
            run = agent.run()
        history = await asyncio.wait_for(run, timeout=cfg.task_timeout)
    except asyncio.TimeoutError:
        history = None
        text = f"Browser-Use didn't finish within {cfg.task_timeout} seconds."
        failed = True
    except BrowserFailure:
        raise
    except Exception as e:  # noqa: BLE001 - Browser-Use's own errors go to the model
        history = None
        text = f"Browser-Use failed: {first_line(e)}"
        failed = True
    finally:
        ticker.cancel()
        try:
            await ticker
        except (asyncio.CancelledError, Exception):  # noqa: BLE001
            pass
    if history is not None:
        text, failed = summarize(history, steps)

    # The page as it ended, then the earlier pages it saw.
    final = await host.frame()
    out: list[bytes] = [d for _, d in shots]
    if final is not None:
        data, url, _ = final
        if shots and shots[-1][0] == url:
            out[-1] = data
        else:
            out.append(data)
    page = host.current_page()
    if page is not None:
        try:
            text += f"\n\nThe browser is now at {page.url}"
        except Exception:  # noqa: BLE001
            pass
    return TaskResult(text=text, shots=out[-KEEP_SHOTS:], is_error=failed)
