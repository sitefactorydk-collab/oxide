# python/

Python that runs in Oxide's managed virtual environment.

- `pyproject.toml` + `uv.lock`: the packages every environment gets (pandas, numpy, matplotlib,
  openpyxl), plus the `browser` dependency group (Browser-Use, the MCP SDK, Playwright) that is
  installed the first time the browser is used. Both files are embedded into the app by
  `crates/oxide-pyenv/build.rs` and written to `<app data>/python/project/` before `uv sync`.
- `oxide_browser_agent/`: the browser agent (Phase 13), an MCP server on stdio. Oxide embeds
  its files too, writes them to `<app data>/python/agents/oxide_browser_agent/` and starts
  `python -m oxide_browser_agent` per chat. Settings come in `OXIDE_BROWSER_*` and
  `OXIDE_LLM_*` environment variables (`config.py`).
- `tests/`: unit tests of the browser agent against a real Chromium and a local web server
  (Browser-Use is replaced by a stand-in):

  ```
  OXIDE_TEST_CHROME=<path to chrome> python -m unittest discover -s python/tests
  ```

Update the lock after changing `pyproject.toml`:

```
node scripts/fetch-uv.mjs          # once: downloads the pinned uv into src-tauri/binaries
src-tauri/binaries/uv-<target>.exe lock --project python
```

(`scripts\win\phase13.ps1` does both.) Without a `uv.lock` the app still works: the first run
resolves the packages itself, which is slower and not reproducible.
