"""Settings from the environment Oxide starts the server with."""

from __future__ import annotations

import os
from dataclasses import dataclass


def _flag(name: str, default: bool) -> bool:
    v = os.environ.get(name)
    if v is None or v.strip() == "":
        return default
    return v.strip().lower() in ("1", "true", "yes", "on")


def _int(name: str, default: int, lo: int, hi: int) -> int:
    try:
        n = int(os.environ.get(name, ""))
    except ValueError:
        return default
    return max(lo, min(hi, n))


@dataclass
class Config:
    # How the browser is reached: a Chromium Oxide starts ("launch") or a running Chrome
    # the user started with remote debugging ("cdp").
    mode: str = "launch"
    # The browser to start; empty means Playwright's own Chromium.
    executable: str = ""
    cdp_url: str = ""
    headless: bool = True
    # Browser-Use's model: Oxide's selected provider.
    llm_kind: str = ""
    llm_base_url: str = ""
    llm_model: str = ""
    llm_api_key: str = ""
    llm_vision: bool = False
    max_steps: int = 25
    task_timeout: int = 600
    # Where searches go: a URL with {q} for the words. Empty: Google, then DuckDuckGo when
    # Google refuses an automated browser.
    search_url: str = ""

    @staticmethod
    def from_env() -> "Config":
        e = os.environ.get
        return Config(
            mode=(e("OXIDE_BROWSER_MODE") or "launch").strip().lower(),
            executable=(e("OXIDE_BROWSER_EXECUTABLE") or "").strip(),
            cdp_url=(e("OXIDE_BROWSER_CDP_URL") or "").strip(),
            headless=_flag("OXIDE_BROWSER_HEADLESS", True),
            llm_kind=(e("OXIDE_LLM_KIND") or "").strip().lower(),
            llm_base_url=(e("OXIDE_LLM_BASE_URL") or "").strip(),
            llm_model=(e("OXIDE_LLM_MODEL") or "").strip(),
            llm_api_key=e("OXIDE_LLM_API_KEY") or "",
            llm_vision=_flag("OXIDE_LLM_VISION", False),
            max_steps=_int("OXIDE_BROWSER_MAX_STEPS", 25, 1, 200),
            task_timeout=_int("OXIDE_BROWSER_TASK_TIMEOUT", 600, 10, 3600),
            search_url=(e("OXIDE_BROWSER_SEARCH_URL") or "").strip(),
        )

    def describe_llm(self) -> str:
        """The model, for messages (never the key)."""
        if not self.llm_model:
            return "no model"
        return f"{self.llm_model} ({self.llm_kind or 'unknown provider'})"
