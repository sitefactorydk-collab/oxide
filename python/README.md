# python/

Python that runs in Oxide's managed virtual environment.

- `pyproject.toml` + `uv.lock`: the packages every environment gets (pandas, numpy, matplotlib,
  openpyxl). Both files are embedded into the app by `crates/oxide-pyenv/build.rs` and written
  to `<app data>/python/project/` before `uv sync`.
- Phase 13 adds `oxide_browser_agent/` here.

Update the lock after changing `pyproject.toml`:

```
node scripts/fetch-uv.mjs          # once: downloads the pinned uv into src-tauri/binaries
src-tauri/binaries/uv-<target>.exe lock --project python
```

(`scripts\win\phase12.ps1` does both.) Without a `uv.lock` the app still works: the first run
resolves the packages itself, which is slower and not reproducible.
