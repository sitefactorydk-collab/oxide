//! The sandbox against a real Docker engine. Ignored by default (needs Docker and downloads
//! images); run with:
//!
//! ```text
//! cargo test -p oxide-sandbox --test real_docker -- --ignored --test-threads 1
//! ```
//!
//! `OXIDE_SANDBOX_IMAGE` and `OXIDE_SANDBOX_FILES_IMAGE` pick the images (defaults: Oxide's
//! defaults, `python:3.12-bookworm` and `mcp/filesystem`). The command image needs
//! `python3`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use oxide_core::McpContent;
use oxide_core::sandbox::{SandboxSettings, SandboxState, SandboxStorage};
use oxide_sandbox::{NoSink, SandboxError, Sandboxes};
use serde_json::json;

fn settings(storage: SandboxStorage) -> SandboxSettings {
    let mut s = SandboxSettings {
        enabled: true,
        storage,
        cpus: 1,
        memory_mb: 512,
        ..SandboxSettings::default()
    };
    if let Ok(i) = std::env::var("OXIDE_SANDBOX_IMAGE") {
        s.image = i;
    }
    if let Ok(i) = std::env::var("OXIDE_SANDBOX_FILES_IMAGE") {
        s.files_image = i;
    }
    s.normalized()
}

fn chat_id(tag: &str) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("test-{tag}-{n}")
}

fn sandboxes(root: &Path, storage: SandboxStorage) -> Sandboxes {
    let s = Sandboxes::new(root.join("sandboxes"), None, Arc::new(NoSink));
    s.configure(&settings(storage));
    s
}

fn text(r: &oxide_core::McpToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| match c {
            McpContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const LONG: Duration = Duration::from_secs(300);

/// The phase's exit check, in code: a project is made in the sandbox, its test fails, the
/// model's fix (through the file server) makes it pass, and the result is copied out to a
/// shared folder. Nothing of this computer is reachable from inside.
#[tokio::test]
#[ignore = "needs Docker; downloads images"]
async fn build_fix_test_and_copy_out() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let s = sandboxes(tmp.path(), SandboxStorage::Volume);
    let st = s.docker_status(true).await;
    assert_eq!(st.state, oxide_core::DockerState::Ready, "{st:?}");
    s.prepare().await.expect("images download");
    let id = chat_id("flow");

    // A project with a failing test.
    let r = s
        .exec(
            &id,
            "mkdir -p calc && cd calc && printf 'def add(a, b):\\n    return a - b\\n' > calc.py \
             && printf 'import unittest\\nfrom calc import add\\n\\nclass T(unittest.TestCase):\\n    def test_add(self):\\n        self.assertEqual(add(2, 3), 5)\\n\\nunittest.main()\\n' > test_calc.py \
             && python3 test_calc.py",
            "",
            LONG,
        )
        .await
        .expect("runs");
    assert_ne!(r.code, Some(0), "the test should fail first: {r:?}");
    assert!(r.stderr.contains("FAILED"), "{r:?}");
    assert_eq!(s.chat(&id).await.state, SandboxState::Running);

    // The fix goes through the file server, which sees the same workspace.
    // Older images have only `read_file`; newer ones `read_text_file` (and `read_file` as a
    // deprecated alias, which is then hidden). Exactly one read tool is offered either way.
    let tools = s.discover_file_tools().await.expect("tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"write_file"), "{names:?}");
    let reads: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| *n == "read_text_file" || *n == "read_file")
        .collect();
    assert_eq!(reads.len(), 1, "{names:?}");
    let read_tool = reads[0].to_owned();
    let read = s
        .call_file_tool(&id, &read_tool, json!({"path": "/workspace/calc/calc.py"}))
        .await
        .expect("reads");
    assert!(text(&read).contains("a - b"), "{read:?}");
    let wrote = s
        .call_file_tool(
            &id,
            "write_file",
            json!({"path": "/workspace/calc/calc.py", "content": "def add(a, b):\n    return a + b  # fixed\n"}),
        )
        .await
        .expect("writes");
    assert!(!wrote.is_error, "{wrote:?}");
    // The server can't leave the workspace either.
    // (as an error result or a protocol error, depending on the server's version)
    let outside = s
        .call_file_tool(&id, &read_tool, json!({"path": "/etc/hostname"}))
        .await;
    assert!(outside.as_ref().map_or(true, |r| r.is_error), "{outside:?}");

    let r = s
        .exec(&id, "python3 test_calc.py", "calc", LONG)
        .await
        .expect("runs");
    assert_eq!(r.code, Some(0), "{r:?}");
    assert_eq!(r.cwd, "/workspace/calc");

    // Listing.
    let l = s
        .list(&id, "calc", false)
        .await
        .expect("lists")
        .expect("running");
    let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"calc.py") && names.contains(&"test_calc.py"),
        "{names:?}"
    );

    // Copy out to a "shared folder", never over an existing file.
    let shared = tmp.path().join("shared");
    std::fs::create_dir(&shared).expect("mkdir");
    let to = s.copy_out(&id, "calc", &shared).await.expect("copies out");
    assert_eq!(to, shared.join("calc"));
    let code = std::fs::read_to_string(shared.join("calc").join("calc.py")).expect("copied");
    assert!(code.contains("a + b"));
    assert!(matches!(
        s.copy_out(&id, "calc", &shared).await,
        Err(SandboxError::Invalid(_))
    ));
    assert!(s.copy_out(&id, "../etc", &shared).await.is_err());

    // Copy in.
    let host_file = shared.join("data.txt");
    std::fs::write(&host_file, "from the host").expect("write");
    let at = s
        .copy_in(&id, &host_file, "inbox/")
        .await
        .expect("copies in");
    assert_eq!(at, "/workspace/inbox/data.txt");
    let r = s
        .exec(&id, "cat inbox/data.txt", "", LONG)
        .await
        .expect("runs");
    assert_eq!(r.stdout.trim(), "from the host");

    // Isolation: no network, and the host's folders aren't there.
    let r = s
        .exec(
            &id,
            "python3 -c \"import socket; socket.create_connection(('1.1.1.1', 53), 3)\"",
            "",
            LONG,
        )
        .await
        .expect("runs");
    assert_ne!(r.code, Some(0), "no network by default: {r:?}");
    let host_path = shared.to_string_lossy().replace('\\', "/");
    let r = s
        .exec(
            &id,
            &format!("test -e '{host_path}' && echo VISIBLE; grep -c ' /workspace ' /proc/mounts; cat /proc/mounts"),
            "",
            LONG,
        )
        .await
        .expect("runs");
    assert!(!r.stdout.contains("VISIBLE"), "{r:?}");
    assert!(!r.stdout.contains(&host_path), "{r:?}");

    // Symbolic links don't leave the sandbox.
    s.exec(
        &id,
        "mkdir -p linky && ln -s /etc/passwd linky/pw",
        "",
        LONG,
    )
    .await
    .expect("runs");
    let e = s.copy_out(&id, "linky", &shared).await;
    assert!(
        matches!(e, Err(SandboxError::Invalid(ref m)) if m.contains("symbolic")),
        "{e:?}"
    );

    // Reset removes everything; the next use starts empty.
    s.reset(&id).await.expect("resets");
    assert_eq!(s.chat(&id).await.state, SandboxState::None);
    let r = s.exec(&id, "ls -A", "", LONG).await.expect("runs");
    assert_eq!(r.stdout.trim(), "", "{r:?}");
    s.reset(&id).await.expect("resets");
}

#[tokio::test]
#[ignore = "needs Docker; downloads images"]
async fn time_limit_kills_the_whole_command() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let s = sandboxes(tmp.path(), SandboxStorage::Volume);
    s.prepare().await.expect("images download");
    let id = chat_id("timeout");
    let r = s
        .exec(
            &id,
            "sleep 120 & sleep 121; echo never",
            "",
            Duration::from_secs(3),
        )
        .await
        .expect("runs");
    assert!(r.timed_out, "{r:?}");
    assert!(s.running_commands(&id).is_empty());
    let r = s
        .exec(
            &id,
            "for p in /proc/[0-9]*; do tr '\\0' ' ' < $p/cmdline 2>/dev/null; echo; done | grep -c '^sleep 12' || true",
            "",
            LONG,
        )
        .await
        .expect("runs");
    assert_eq!(r.stdout.trim(), "0", "the sleeps were killed: {r:?}");
    // A folder that doesn't exist.
    let r = s.exec(&id, "pwd", "nope", LONG).await.expect("runs");
    assert_ne!(r.code, Some(0));
    assert!(r.stderr.contains("doesn't exist"), "{r:?}");
    // Stop keeps the files; the next command starts it again.
    s.exec(&id, "echo kept > k.txt", "", LONG)
        .await
        .expect("runs");
    s.stop(&id).await.expect("stops");
    assert_eq!(s.chat(&id).await.state, SandboxState::Stopped);
    assert!(s.list(&id, "", false).await.expect("lists").is_none());
    let r = s.exec(&id, "cat k.txt", "", LONG).await.expect("runs");
    assert_eq!(r.stdout.trim(), "kept");
    s.reset(&id).await.expect("resets");
}

#[tokio::test]
#[ignore = "needs Docker; downloads images"]
async fn folder_workspaces_and_new_settings() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let s = sandboxes(tmp.path(), SandboxStorage::Folder);
    s.prepare().await.expect("images download");
    let id = chat_id("folder");
    s.exec(&id, "echo hi > hello.txt", "", LONG)
        .await
        .expect("runs");
    let chat = s.chat(&id).await;
    assert_eq!(chat.storage, SandboxStorage::Folder);
    let folder = std::path::PathBuf::from(chat.workspace.expect("a folder"));
    assert!(folder.starts_with(tmp.path()), "{folder:?}");
    assert_eq!(
        std::fs::read_to_string(folder.join("hello.txt"))
            .expect("on disk")
            .trim(),
        "hi"
    );
    // New limits make a new container; the workspace stays.
    let mut next = s.settings();
    next.memory_mb = 768;
    s.configure(&next);
    let r = s.exec(&id, "cat hello.txt", "", LONG).await.expect("runs");
    assert_eq!(r.stdout.trim(), "hi");
    s.reset(&id).await.expect("resets");
    assert!(!folder.exists(), "reset removes the chat's folder");
}
