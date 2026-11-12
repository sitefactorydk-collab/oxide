"""Oxide's browser agent: an MCP server (stdio) that drives a Chromium browser.

Oxide starts one per chat, in its managed Python environment, with ``python -m
oxide_browser_agent``. Settings arrive as environment variables (see ``config.py``).
"""

__version__ = "0.1.0"
