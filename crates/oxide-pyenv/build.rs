//! Copies `python/pyproject.toml` and `python/uv.lock` (if there is one) into `OUT_DIR`, so the
//! crate can embed them. A missing lock file embeds an empty one: the first run then resolves
//! the packages itself.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR"),
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo always sets OUT_DIR"));
    let python = manifest.join("..").join("..").join("python");
    let pyproject = python.join("pyproject.toml");
    let lock = python.join("uv.lock");
    println!("cargo:rerun-if-changed={}", pyproject.display());
    println!("cargo:rerun-if-changed={}", lock.display());
    println!("cargo:rerun-if-changed=build.rs");

    let text = std::fs::read_to_string(&pyproject)
        .unwrap_or_else(|e| panic!("python/pyproject.toml is missing or unreadable: {e}"));
    std::fs::write(out.join("pyproject.toml"), text).expect("OUT_DIR is writable");
    let lock_text = std::fs::read_to_string(&lock).unwrap_or_default();
    if lock_text.trim().is_empty() {
        println!(
            "cargo:warning=python/uv.lock is missing: run scripts\\win\\phase12.ps1 (or `uv lock --project python`) and commit it"
        );
    }
    std::fs::write(out.join("uv.lock"), lock_text).expect("OUT_DIR is writable");
}
