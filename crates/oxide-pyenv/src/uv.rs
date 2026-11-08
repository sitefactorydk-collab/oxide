//! Finding Oxide's `uv`.
//!
//! The release build bundles `uv` as a Tauri sidecar (`binaries/uv-<target>`; the bundler
//! puts it next to the app as plain `uv`). Development builds find it in `src-tauri/binaries`
//! (filled by `npm run fetch-uv`). As a last resort, a `uv` on the PATH is used.

use std::path::{Path, PathBuf};

/// The Rust target triple of this build, as `uv`'s release assets and Tauri sidecars name it.
pub fn target_triple() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(windows, target_arch = "aarch64")) {
        "aarch64-pc-windows-msvc"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

/// `uv.exe` or `uv`.
pub fn exe_name() -> &'static str {
    if cfg!(windows) { "uv.exe" } else { "uv" }
}

fn usable(p: &Path) -> bool {
    // A zero-length file is the placeholder a development build creates when the real
    // sidecar hasn't been fetched.
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
}

/// The first usable `uv`: `explicit`, then each of `dirs` (as `uv` and as `uv-<target>`),
/// then the PATH.
pub fn find(explicit: Option<&Path>, dirs: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = explicit
        && usable(p)
    {
        return Some(p.to_path_buf());
    }
    let sidecar = format!(
        "uv-{}{}",
        target_triple(),
        if cfg!(windows) { ".exe" } else { "" }
    );
    for dir in dirs {
        for name in [exe_name(), sidecar.as_str()] {
            let p = dir.join(name);
            if usable(&p) {
                return Some(p);
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe_name()))
        .find(|p| usable(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_uv_in_given_folders_and_skips_placeholders() {
        let tmp = tempfile::tempdir().expect("tmp");
        let empty = tmp.path().join("a");
        let real = tmp.path().join("b");
        std::fs::create_dir_all(&empty).expect("mkdir");
        std::fs::create_dir_all(&real).expect("mkdir");
        std::fs::write(empty.join(exe_name()), b"").expect("placeholder");
        let sidecar = format!(
            "uv-{}{}",
            target_triple(),
            if cfg!(windows) { ".exe" } else { "" }
        );
        std::fs::write(real.join(sidecar), b"binary").expect("sidecar");
        let found = find(None, &[empty, real.clone()]).expect("found");
        assert!(found.starts_with(&real), "{found:?}");
        let explicit = tmp.path().join("mine");
        std::fs::write(&explicit, b"x").expect("write");
        assert_eq!(find(Some(&explicit), &[]), Some(explicit));
    }
}
