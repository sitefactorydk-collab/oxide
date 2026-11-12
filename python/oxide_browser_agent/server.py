"""The MCP server (stdio): ``browser_task`` plus granular browser tools.

Tools return ``CallToolResult`` directly: text for the model, JPEG screenshots as images,
and ``is_error`` for failures the model should hear about.
"""

from __future__ import annotations

import asyncio
import base64
import sys
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Annotated, Any

from mcp.server.mcpserver import Context, MCPServer
from mcp_types import CallToolResult, ImageContent, TextContent, ToolAnnotations
from pydantic import Field

from . import __version__
from .agent import progress_message, run_task
from .config import Config
from .host import BrowserFailure, BrowserHost, first_line
from .pagetext import slice_text

INSTRUCTIONS = (
    "A web browser on the user's computer. Use search to find pages on Google (don't guess "
    "addresses), navigate to open a page, read_page to read it "
    "(links, buttons and fields come numbered, like [3] button \"Search\"), click and type "
    "with those numbers, and screenshot to see it. browser_task hands a whole goal to an "
    "autonomous browsing agent."
)

# Text that comes with navigate/click/type results; read_page gives the rest.
ACTION_TEXT_CHARS = 6_000
READ_DEFAULT_CHARS = 20_000
READ_MAX_CHARS = 100_000

CFG = Config.from_env()
HOST = BrowserHost(CFG)
# One browser action at a time (they share the page).
LOCK = asyncio.Lock()

@asynccontextmanager
async def lifespan(_server: Any) -> AsyncIterator[None]:
    try:
        yield None
    finally:
        # Oxide stopped the server: close the browser with it.
        if HOST.started:
            try:
                await HOST.close()
            except Exception:  # noqa: BLE001
                pass


mcp = MCPServer(
    "oxide-browser-agent", version=__version__, instructions=INSTRUCTIONS, lifespan=lifespan
)


def text_result(text: str, is_error: bool = False) -> CallToolResult:
    return CallToolResult(content=[TextContent(type="text", text=text)], is_error=is_error)


def page_summary(info: dict[str, Any], chars: int, start: int = 0) -> str:
    head = [f"Note: {info['note']}", ""] if info.get("note") else []
    head += [f"Title: {info.get('title') or '(none)'}", f"URL: {info.get('url') or ''}"]
    if info.get("status") and int(info["status"]) >= 400:
        head.append(f"HTTP status: {info['status']}")
    body = info.get("text") or "(The page has no visible text.)"
    overview = ""
    if info.get("overview") and start == 0:
        overview = (
            "Google's AI Overview (an AI-written summary at the top of the results; use it when "
            "it answers the question, say that it comes from Google's AI Overview, and check "
            "its sources when accuracy matters):\n"
            f"{info['overview']}\n\n--- The results page ---\n"
        )
    return "\n".join(head) + "\n\n" + overview + slice_text(body, start, chars)


def image(data: bytes) -> ImageContent:
    return ImageContent(type="image", data=base64.b64encode(data).decode("ascii"), mime_type="image/jpeg")


async def send_frame(ctx: Context | None, text: str) -> None:
    """One frame of the live view after an action (best effort)."""
    if ctx is None:
        return
    got = await HOST.frame()
    if got is None:
        return
    data, url, _title = got
    try:
        await ctx.report_progress(1, None, progress_message(text, data, url))
    except Exception:  # noqa: BLE001 - progress is optional
        pass


async def guarded(work) -> CallToolResult:
    async with LOCK:
        try:
            return await work()
        except BrowserFailure as e:
            return text_result(str(e), is_error=True)
        except Exception as e:  # noqa: BLE001 - anything else still answers the call
            print(f"[oxide-browser] {e!r}", file=sys.stderr, flush=True)
            return text_result(f"The browser tool failed: {first_line(e)}", is_error=True)


@mcp.tool(
    title="Browse autonomously",
    description=(
        "Hand a browsing goal to an autonomous agent (Browser-Use) that opens pages, clicks, "
        "types and reads until it is done, then reports what it found with screenshots. Good "
        "for multi-step tasks on websites. For simple look-ups, navigate and read_page are "
        "faster. Say exactly what to find or do and what to report back."
    ),
    annotations=ToolAnnotations(read_only_hint=False, open_world_hint=True),
)
async def browser_task(
    goal: Annotated[str, Field(description="What to do and what to report, in plain words.")],
    ctx: Context,
    max_steps: Annotated[
        int | None, Field(description="Most browser steps (default from Oxide's settings).", ge=1, le=200)
    ] = None,
) -> CallToolResult:
    async def work() -> CallToolResult:
        async def report(n: float, message: str) -> None:
            await ctx.report_progress(n, None, message)

        result = await run_task(HOST, CFG, goal, max_steps, report)
        content: list[Any] = [TextContent(type="text", text=result.text)]
        content.extend(image(s) for s in result.shots)
        return CallToolResult(content=content, is_error=result.is_error)

    return await guarded(work)


@mcp.tool(
    title="Open a web page",
    description=(
        "Open a URL in the browser and return the page's title, address and the start of its "
        "text, with links, buttons and fields numbered for click and type. Search words "
        "instead of a URL are searched on Google. An address that doesn't exist (unknown site "
        "or 404) is searched for automatically, so open a real result rather than guessing "
        "another address."
    ),
    annotations=ToolAnnotations(read_only_hint=False, open_world_hint=True),
)
async def navigate(
    url: Annotated[
        str,
        Field(description="The address, e.g. https://example.com, or search words."),
    ],
    ctx: Context,
) -> CallToolResult:
    async def work() -> CallToolResult:
        info = await HOST.navigate(url)
        await send_frame(ctx, f"Opened {info.get('title') or info.get('url')}")
        return text_result(page_summary(info, ACTION_TEXT_CHARS))

    return await guarded(work)


@mcp.tool(
    title="Search Google",
    description=(
        "Search the web on Google in the browser and return the results page, with each "
        "result's link numbered for click (and Google's AI Overview first, when there is one). Use it to find the right page whenever you don't "
        "know its exact address, instead of guessing URLs."
    ),
    annotations=ToolAnnotations(read_only_hint=True, open_world_hint=True),
)
async def search(
    query: Annotated[str, Field(description="What to search for, in plain words.")],
    ctx: Context,
) -> CallToolResult:
    async def work() -> CallToolResult:
        words = (query or "").strip()
        if not words:
            return text_result("search needs a query.", is_error=True)
        info = await HOST.search(words)
        info["note"] = f'Results for "{words}". Open one by its number with click.'
        await send_frame(ctx, f"Searched for {words}")
        return text_result(page_summary(info, ACTION_TEXT_CHARS))

    return await guarded(work)


@mcp.tool(
    title="Read the page",
    description=(
        "Read the current page as text: headings, text, and numbered links, buttons and "
        "fields ([3] button \"Search\"). Numbers change when the page changes, so read again "
        "after clicking. Long pages are cut; pass start to read on."
    ),
    annotations=ToolAnnotations(read_only_hint=True, open_world_hint=True),
)
async def read_page(
    start: Annotated[int, Field(description="Character to start at.", ge=0)] = 0,
    max_chars: Annotated[
        int, Field(description="Most characters to return.", ge=1_000, le=READ_MAX_CHARS)
    ] = READ_DEFAULT_CHARS,
) -> CallToolResult:
    async def work() -> CallToolResult:
        if not HOST.started:
            return text_result("No page is open yet. Use navigate first.", is_error=True)
        info = await HOST.page_text()
        return text_result(page_summary(info, max_chars, start))

    return await guarded(work)


@mcp.tool(
    title="Click",
    description=(
        "Click a link, button or other element: by its number from read_page (ref), or by "
        "its visible text. Returns the page after the click."
    ),
    annotations=ToolAnnotations(read_only_hint=False, open_world_hint=True),
)
async def click(
    ctx: Context,
    ref: Annotated[int | None, Field(description="The element's number from read_page.")] = None,
    text: Annotated[str | None, Field(description="Visible text of the element, when there is no number.")] = None,
) -> CallToolResult:
    async def work() -> CallToolResult:
        if not HOST.started:
            return text_result("No page is open yet. Use navigate first.", is_error=True)
        info = await HOST.click(ref, text)
        await send_frame(ctx, f"Clicked {f'[{ref}]' if ref is not None else text}")
        return text_result("Clicked.\n\n" + page_summary(info, ACTION_TEXT_CHARS))

    return await guarded(work)


@mcp.tool(
    name="type",
    title="Type",
    description=(
        "Type into a text field (by its number from read_page). Replaces what is there "
        "unless clear is false; submit presses Enter afterwards. Returns the page."
    ),
    annotations=ToolAnnotations(read_only_hint=False, open_world_hint=True),
)
async def type_text(
    text: Annotated[str, Field(description="What to type.")],
    ctx: Context,
    ref: Annotated[int | None, Field(description="The field's number from read_page.")] = None,
    field: Annotated[
        str | None, Field(description="Visible text or placeholder of the field, when there is no number.")
    ] = None,
    submit: Annotated[bool, Field(description="Press Enter afterwards.")] = False,
    clear: Annotated[bool, Field(description="Replace the field's text (default) instead of adding to it.")] = True,
) -> CallToolResult:
    async def work() -> CallToolResult:
        if not HOST.started:
            return text_result("No page is open yet. Use navigate first.", is_error=True)
        info = await HOST.type(ref, text, submit, clear, field)
        await send_frame(ctx, "Typed" + (" and submitted" if submit else ""))
        done = "Typed and pressed Enter." if submit else "Typed."
        return text_result(done + "\n\n" + page_summary(info, ACTION_TEXT_CHARS))

    return await guarded(work)


@mcp.tool(
    title="Screenshot",
    description="Take a screenshot of the current page (the visible part, or the whole page).",
    annotations=ToolAnnotations(read_only_hint=True, open_world_hint=False),
)
async def screenshot(
    full_page: Annotated[bool, Field(description="The whole page instead of the visible part.")] = False,
) -> CallToolResult:
    async def work() -> CallToolResult:
        if not HOST.started:
            return text_result("No page is open yet. Use navigate first.", is_error=True)
        data = await HOST.screenshot(full_page=full_page)
        page = await HOST.page()
        title = await page.title()
        return CallToolResult(
            content=[TextContent(type="text", text=f"{title}\n{page.url}"), image(data)]
        )

    return await guarded(work)


@mcp.tool(
    title="Close the browser",
    description="Close the browser when you are done with it (the next navigate opens it again).",
    annotations=ToolAnnotations(read_only_hint=False, destructive_hint=False, open_world_hint=False),
)
async def close() -> CallToolResult:
    async def work() -> CallToolResult:
        return text_result(await HOST.close())

    return await guarded(work)


def main() -> None:
    mcp.run("stdio")
