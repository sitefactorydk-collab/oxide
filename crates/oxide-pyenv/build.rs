//! Copies `python/pyproject.toml` and `python/uv.lock` (if there is one) into `OUT_DIR`, so the
//! crate can embed them. A missing lock file embeds an empty one: the first run then resolves
//! the packages itself.
//!
//! Also embeds the browser agent's files (`python/oxide_browser_agent/*.py`) as a table,
//! `agent_files.rs`.

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

    let agent = python.join("oxide_browser_agent");
    println!("cargo:rerun-if-changed={}", agent.display());
    let mut names: Vec<String> = std::fs::read_dir(&agent)
        .unwrap_or_else(|e| panic!("python/oxide_browser_agent is missing: {e}"))
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| n.ends_with(".py"))
        .collect();
    names.sort();
    let copy = out.join("oxide_browser_agent");
    std::fs::create_dir_all(&copy).expect("OUT_DIR is writable");
    let mut table = String::from("pub(crate) const AGENT_FILES: &[(&str, &str)] = &[\n");
    for n in &names {
        let src = agent.join(n);
        println!("cargo:rerun-if-changed={}", src.display());
        let text = std::fs::read_to_string(&src)
            .unwrap_or_else(|e| panic!("{} is unreadable: {e}", src.display()));
        std::fs::write(copy.join(n), text).expect("OUT_DIR is writable");
        table.push_str(&format!(
            "    (\"{n}\", include_str!(concat!(env!(\"OUT_DIR\"), \"/oxide_browser_agent/{n}\"))),\n"
        ));
    }
    table.push_str("];\n");
    std::fs::write(out.join("agent_files.rs"), table).expect("OUT_DIR is writable");
}
