//! Managed Python for Oxide (Phase 12): finds Oxide's `uv`, installs CPython and a fixed set of
//! packages into a virtual environment in the app's data folder, and runs scripts in it.
//!
//! No Tauri here: progress goes to a [`PythonSink`], which the app turns into an event.

mod env;
mod error;
mod files;
mod process;
mod project;
mod run;
pub mod uv;

pub use env::{NoSink, PythonConfig, PythonEnv, PythonSink, Ready};
pub use error::{PythonError, Result};
pub use files::{ChangedFile, Snapshot};
pub use run::{RunOutput, RunRequest};

#[cfg(unix)]
impl PythonEnv {
    /// An environment that looks installed and runs scripts with `system_python`, for tests
    /// of code that uses it (no `uv`, no packages).
    #[doc(hidden)]
    pub fn for_tests(root: &std::path::Path, system_python: &std::path::Path) -> Self {
        let env = Self::new(PythonConfig::new(root), std::sync::Arc::new(NoSink));
        let python = env.venv_python();
        std::fs::create_dir_all(python.parent().expect("the venv python has a folder"))
            .expect("the test folder is writable");
        std::os::unix::fs::symlink(system_python, &python).expect("symlinks work in tests");
        let marker = serde_json::json!({
            "schema": 1,
            "projectHash": project::project_hash(),
            "pythonVersion": "3.12.0",
            "uvVersion": "uv test",
            "packages": [],
        });
        std::fs::write(root.join("state.json"), marker.to_string())
            .expect("the test folder is writable");
        env
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use oxide_core::{PythonState, PythonStatus};

    use super::*;

    fn system_python() -> Option<std::path::PathBuf> {
        let out = std::process::Command::new("sh")
            .args(["-c", "command -v python3"])
            .output()
            .ok()?;
        let p = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (!p.is_empty()).then(|| std::path::PathBuf::from(p))
    }

    fn req(tmp: &std::path::Path, code: &str, secs: u64) -> RunRequest {
        RunRequest {
            code: code.to_owned(),
            cwd: tmp.join("cwd"),
            timeout: Duration::from_secs(secs),
        }
    }

    #[tokio::test]
    async fn scripts_run_in_their_folder_with_output_and_exit_code() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let env = PythonEnv::for_tests(tmp.path(), &py);
        let before = Snapshot::take(&tmp.path().join("cwd"));
        let code = "import os, sys\nprint('hej æøå', os.path.basename(os.getcwd()))\n\
                    open('out.csv', 'w').write('a,b\\n1,2\\n')\nprint('oops', file=sys.stderr)\n";
        let out = env.run(req(tmp.path(), code, 30)).await.expect("runs");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.exit_code,
            Some(0),
            "{stdout} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(stdout.contains("hej æøå cwd"), "{stdout}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("oops"));
        let cwd = tmp.path().join("cwd");
        let changed = before.changed_since(&cwd, &Snapshot::take(&cwd));
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].rel, "out.csv");
        // The script's own folder is cleaned up.
        let left: Vec<_> = std::fs::read_dir(tmp.path().join("runs"))
            .map(|r| r.flatten().collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
    }

    #[tokio::test]
    async fn errors_show_the_scripts_own_frames_and_exit_codes_pass_through() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let env = PythonEnv::for_tests(tmp.path(), &py);
        let out = env
            .run(req(tmp.path(), "x = 1\nraise ValueError('nope')\n", 30))
            .await
            .expect("runs");
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.exit_code, Some(1));
        assert!(
            err.contains("ValueError: nope") && err.contains("line 2"),
            "{err}"
        );
        assert!(
            !err.contains("oxide_run.py") && !err.contains("runpy"),
            "{err}"
        );
        let out = env
            .run(req(tmp.path(), "import sys\nsys.exit(3)\n", 30))
            .await
            .expect("runs");
        assert_eq!(out.exit_code, Some(3));
        let out = env
            .run(req(tmp.path(), "raise SystemExit('bye')\n", 30))
            .await
            .expect("runs");
        assert_eq!(out.exit_code, Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains("bye"));
    }

    #[tokio::test]
    async fn the_time_limit_stops_a_script_and_its_children() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let env = PythonEnv::for_tests(tmp.path(), &py);
        let started = std::time::Instant::now();
        let code = "import subprocess, time\nsubprocess.Popen(['sleep', '30'])\ntime.sleep(30)\n";
        let out = env
            .run(RunRequest {
                code: code.into(),
                cwd: tmp.path().join("cwd"),
                timeout: Duration::from_millis(500),
            })
            .await
            .expect("runs");
        assert!(out.timed_out && out.exit_code.is_none());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn open_matplotlib_figures_are_saved_unless_the_script_saved_an_image() {
        let Some(py) = system_python() else { return };
        let probe = std::process::Command::new(&py)
            .args(["-c", "import matplotlib"])
            .status();
        if !probe.is_ok_and(|s| s.success()) {
            return;
        }
        let tmp = tempfile::tempdir().expect("tmp");
        let env = PythonEnv::for_tests(tmp.path(), &py);
        let cwd = tmp.path().join("cwd");
        let code = "import matplotlib.pyplot as plt\nplt.plot([1, 2, 3])\nplt.show()\n";
        let out = env.run(req(tmp.path(), code, 120)).await.expect("runs");
        assert_eq!(
            out.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(cwd.join("figure-1.png").is_file());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("non-interactive"));
        // The script saved its own image: no extra figure.
        let code = "import matplotlib.pyplot as plt\nplt.plot([1])\nplt.savefig('mine.png')\n";
        env.run(req(tmp.path(), code, 120)).await.expect("runs");
        assert!(cwd.join("mine.png").is_file());
        assert!(!cwd.join("figure-2.png").exists());
    }

    #[tokio::test]
    async fn work_folders_are_per_chat_and_forgotten() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let env = PythonEnv::for_tests(tmp.path(), &py);
        let dir = env.work_dir("chat/../1");
        assert!(dir.starts_with(tmp.path().join("work")) && !dir.to_string_lossy().contains(".."));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("f.txt"), "x").expect("write");
        env.forget_chat("chat/../1").await;
        assert!(!dir.exists());
    }

    struct Collect(Mutex<Vec<PythonStatus>>);
    impl PythonSink for Collect {
        fn changed(&self, s: &PythonStatus) {
            self.0.lock().expect("lock").push(s.clone());
        }
    }

    /// A `uv` that does just enough: python install, venv (a link to a real Python), sync.
    fn fake_uv(
        dir: &std::path::Path,
        python: &std::path::Path,
        fail_sync: bool,
    ) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("uv");
        let sync = if fail_sync {
            "echo 'error: no network' >&2; exit 2"
        } else {
            "echo synced"
        };
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  --version) echo 'uv 9.9.9' ;;\n  python) echo 'installed' ;;\n  \
             venv) mkdir -p \"$2/bin\" && ln -s '{}' \"$2/bin/python\" && echo \"created $2\" ;;\n  \
             sync) {sync} ;;\n  *) echo \"unknown $1\" >&2; exit 2 ;;\nesac\n",
            python.display()
        );
        std::fs::write(&path, script).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    #[tokio::test]
    async fn setup_walks_the_steps_reports_progress_and_marks_the_environment_ready() {
        let Some(py) = system_python() else { return };
        if !std::process::Command::new(&py)
            .args(["-c", "import pip"])
            .status()
            .is_ok_and(|s| s.success())
        {
            return;
        }
        let tmp = tempfile::tempdir().expect("tmp");
        let sink = Arc::new(Collect(Mutex::new(Vec::new())));
        let mut cfg = PythonConfig::new(tmp.path().join("python"));
        cfg.uv = Some(fake_uv(tmp.path(), &py, false));
        cfg.packages = vec!["pip".into()];
        let env = PythonEnv::new(cfg, sink.clone());
        assert_eq!(env.status(false).await.state, PythonState::Missing);

        // Two callers at once share one installation.
        let (a, b) = tokio::join!(env.ensure(), env.ensure());
        a.expect("first");
        b.expect("second");
        let st = env.status(true).await;
        assert_eq!(st.state, PythonState::Ready, "{st:?}");
        assert!(st.packages.iter().any(|p| p.name == "pip"));
        assert_eq!(st.uv_version.as_deref(), Some("uv 9.9.9"));
        assert!(st.disk_bytes.is_some());

        let seen = sink.0.lock().expect("lock").clone();
        let steps: Vec<_> = seen.iter().filter_map(|s| s.step).collect();
        for want in oxide_core::PythonStep::ALL {
            assert!(steps.contains(&want), "missing {want:?} in {steps:?}");
        }
        assert!(seen.iter().any(|s| s.state == PythonState::Installing));
        assert!(
            seen.iter()
                .any(|s| s.log.iter().any(|l| l.contains("synced")))
        );
        // The install ran uv's own project files and the launcher.
        assert!(tmp.path().join("python/project/pyproject.toml").is_file());
        assert!(tmp.path().join("python/runner/oxide_run.py").is_file());

        // A script runs in it; repair rebuilds; reset removes it all but chats' folders.
        std::fs::create_dir_all(env.work_dir("c1")).expect("work");
        let out = env
            .run(req(tmp.path(), "print(1 + 1)", 30))
            .await
            .expect("runs");
        assert!(String::from_utf8_lossy(&out.stdout).contains('2'));
        env.repair().await.expect("repair");
        assert_eq!(env.status(false).await.state, PythonState::Ready);
        env.reset().await.expect("reset");
        assert_eq!(env.status(false).await.state, PythonState::Missing);
        assert!(env.work_dir("c1").is_dir());
        assert!(!env.venv_dir().exists());
    }

    #[tokio::test]
    async fn a_failing_step_is_reported_with_uvs_last_lines_and_can_be_retried() {
        let Some(py) = system_python() else { return };
        let tmp = tempfile::tempdir().expect("tmp");
        let mut cfg = PythonConfig::new(tmp.path().join("python"));
        cfg.uv = Some(fake_uv(tmp.path(), &py, true));
        let env = PythonEnv::new(cfg, Arc::new(NoSink));
        let err = env.ensure().await.expect_err("sync fails").to_string();
        assert!(
            err.contains("Installing packages") && err.contains("no network"),
            "{err}"
        );
        let st = env.status(false).await;
        assert_eq!(st.state, PythonState::Failed);
        assert!(
            st.error
                .as_deref()
                .is_some_and(|e| e.contains("no network"))
        );
        assert!(st.log.iter().any(|l| l.contains("no network")));
    }

    #[tokio::test]
    async fn without_uv_the_error_says_what_to_do() {
        let tmp = tempfile::tempdir().expect("tmp");
        let mut cfg = PythonConfig::new(tmp.path().join("python"));
        // No PATH lookup can find a uv in an empty folder list... unless the machine has one,
        // so point at a missing explicit file and an empty folder list and check the message.
        cfg.uv = Some(tmp.path().join("missing"));
        let env = PythonEnv::new(cfg, Arc::new(NoSink));
        if env.find_uv().is_some() {
            return; // this machine has uv on its PATH
        }
        let err = env.ensure().await.expect_err("no uv").to_string();
        assert!(err.contains("npm run fetch-uv"), "{err}");
    }
}
