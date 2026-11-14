/// Tauri requires the `uv` sidecar for the target to exist (`binaries/uv-<triple>[.exe]`).
/// `npm run fetch-uv` downloads it. A debug build that doesn't have it yet gets an empty
/// placeholder (Oxide ignores it and says how to fetch the real one when Python is first
/// needed); a release build fails, so an installer never ships without `uv`.
fn ensure_uv_sidecar() {
    let triple = std::env::var("TARGET").expect("cargo always sets TARGET");
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
    let ext = if triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let path = std::path::Path::new(&manifest_dir)
        .join("binaries")
        .join(format!("uv-{triple}{ext}"));
    println!("cargo:rerun-if-changed={}", path.display());
    let present = std::fs::metadata(&path).is_ok_and(|m| m.len() > 0);
    if present {
        return;
    }
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        panic!(
            "the uv sidecar {} is missing: run `npm run fetch-uv` before a release build",
            path.display()
        );
    }
    println!(
        "cargo:warning=the uv sidecar {} is missing (run `npm run fetch-uv`); Python tools won't work",
        path.display()
    );
    if !path.exists() {
        // Best effort: an empty file satisfies the bundler's existence check.
        let _ = std::fs::create_dir_all(path.parent().expect("binaries has a parent"));
        let _ = std::fs::write(&path, b"");
    }
}

/// Oxide's own command list (`collect_commands!` in `src/lib.rs`), shared with the tests.
#[path = "src/acl.rs"]
mod acl;

/// The built Chrome extension is bundled as a resource (`bundle.resources`). Debug builds
/// get an empty folder when it hasn't been built (`npm run extension`); release builds fail.
fn ensure_extension_dist() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
    let dist = std::path::Path::new(&manifest_dir)
        .join("..")
        .join("extension")
        .join("dist");
    println!(
        "cargo:rerun-if-changed={}",
        dist.join("manifest.json").display()
    );
    if dist.join("manifest.json").is_file() {
        return;
    }
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        panic!(
            "the Chrome extension isn't built ({} has no manifest.json): run `npm run extension` before a release build",
            dist.display()
        );
    }
    println!(
        "cargo:warning=the Chrome extension isn't built (run `npm run extension`); the app bundles an empty folder"
    );
    // Tauri's resource check needs at least one file; Oxide only uses the folder when it
    // has a manifest.json.
    let _ = std::fs::create_dir_all(&dist);
    let _ = std::fs::write(
        dist.join("NOT-BUILT.txt"),
        "Run `npm run extension` to build the Chrome extension.\n",
    );
}

fn main() {
    ensure_uv_sidecar();
    ensure_extension_dist();

    // On Windows/MSVC we embed our own app manifest through the linker (below) instead of letting
    // tauri-build do it, because tauri-build only adds it to the main binary. Test executables need
    // it too, or they crash on start with STATUS_ENTRYPOINT_NOT_FOUND.
    let is_windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");

    // Declaring the app's commands turns on Tauri's access check for them: a window may
    // call only those its capability allows (`capabilities/*.json`).
    println!("cargo:rerun-if-changed=src/lib.rs");
    let lib_rs = std::fs::read_to_string("src/lib.rs").expect("src/lib.rs is readable");
    let commands: Vec<&'static str> = acl::commands_in(&lib_rs)
        .into_iter()
        .map(|c| &*Box::leak(c.into_boxed_str()))
        .collect();
    assert!(
        !commands.is_empty(),
        "no commands found in collect_commands! in src/lib.rs"
    );
    let commands: &'static [&'static str] = Box::leak(commands.into_boxed_slice());

    let mut attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(commands));
    if is_windows_msvc {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    if let Err(err) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {err:#}");
    }

    if is_windows_msvc {
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").expect("cargo always sets CARGO_MANIFEST_DIR");
        let manifest = std::path::Path::new(&manifest_dir).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
