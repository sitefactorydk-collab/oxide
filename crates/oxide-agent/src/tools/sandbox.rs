//! The Docker sandbox's tools (Phase 12b): `sandbox_exec`, `sandbox_copy_in`,
//! `sandbox_copy_out`, and the filesystem server's tools as `sandbox__<tool>`.
//!
//! The work happens in `oxide-sandbox`; this module describes the tools to models, checks
//! their input, and writes their results. Copies reach this computer only inside the chat's
//! shared folders (the Phase 11 [`Sandbox`] confines them, like the file tools).

use std::time::Duration;

use oxide_core::McpTool;
use oxide_core::sandbox::{
    SANDBOX_COPY_IN, SANDBOX_COPY_OUT, SANDBOX_EXEC, SANDBOX_SERVER, WORKSPACE_DIR, command_label,
};
use oxide_sandbox::{ExecResult, SandboxError, Sandboxes};
use serde_json::{Value, json};

use super::files::{Sandbox, display_path};
use super::{Builtin, ToolEntry, ToolKind, api_name, normalize_schema};

/// Most characters of stdout (and of stderr) a result shows.
pub const MAX_OUTPUT_CHARS: usize = 30_000;
/// Longest command, in characters.
const MAX_COMMAND_CHARS: usize = 10_000;

pub fn exec_entry() -> ToolEntry {
    ToolEntry {
        api_name: SANDBOX_EXEC.into(),
        kind: ToolKind::Builtin(Builtin::SandboxExec),
        title: Some("Run in the sandbox".into()),
        description: format!(
            "Run a shell command (bash) in this chat's sandbox: an isolated Linux container \
             whose working folder {WORKSPACE_DIR} keeps its files between commands. Use it to \
             create, build, run and test code: install packages, run compilers, test suites \
             and scripts. The sandbox can't see the user's computer; move files in and out \
             with sandbox_copy_in and sandbox_copy_out. Output is cut to the start and end \
             when long. Commands have a time limit, so don't start servers or anything that \
             runs forever in the foreground."
        ),
        schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command line (bash). Chain steps with && ." },
                "cwd": { "type": "string", "description": format!("Folder to run in, relative to {WORKSPACE_DIR} (default: {WORKSPACE_DIR}).") }
            },
            "required": ["command"]
        }),
        read_only: false,
        destructive: false,
    }
}

pub fn copy_in_entry() -> ToolEntry {
    ToolEntry {
        api_name: SANDBOX_COPY_IN.into(),
        kind: ToolKind::Builtin(Builtin::SandboxCopyIn),
        title: Some("Copy into the sandbox".into()),
        description: format!(
            "Copy a file or folder from a folder the user shared with this chat into the \
             sandbox. The user approves each copy. `to` is a path in the sandbox; an existing \
             folder there (or a path ending in /) receives it under its own name. Default: \
             {WORKSPACE_DIR}/."
        ),
        schema: json!({
            "type": "object",
            "properties": {
                "from": { "type": "string", "description": "The file or folder on the user's computer, inside a shared folder." },
                "to": { "type": "string", "description": format!("Where it goes in the sandbox (default {WORKSPACE_DIR}/).") }
            },
            "required": ["from"]
        }),
        read_only: false,
        destructive: false,
    }
}

pub fn copy_out_entry() -> ToolEntry {
    ToolEntry {
        api_name: SANDBOX_COPY_OUT.into(),
        kind: ToolKind::Builtin(Builtin::SandboxCopyOut),
        title: Some("Copy out of the sandbox".into()),
        description: "Copy a file or folder from the sandbox into a folder the user shared \
             with this chat. The user approves each copy. `to` is an existing folder (the copy \
             keeps its name) or a new path; nothing that exists is overwritten, and symbolic \
             links aren't copied."
            .into(),
        schema: json!({
            "type": "object",
            "properties": {
                "from": { "type": "string", "description": format!("The file or folder in the sandbox (relative to {WORKSPACE_DIR} or absolute).") },
                "to": { "type": "string", "description": "Where it goes on the user's computer, inside a shared folder (default: the first shared folder)." }
            },
            "required": ["from"]
        }),
        read_only: false,
        destructive: false,
    }
}

/// The filesystem server's tools as models get them (`sandbox__write_file`, …).
pub fn file_entries(tools: &[McpTool]) -> Vec<ToolEntry> {
    tools
        .iter()
        .map(|t| {
            let description = t.description.as_deref().unwrap_or("").trim();
            let read_only = t.annotations.read_only_hint == Some(true);
            ToolEntry {
                api_name: api_name(SANDBOX_SERVER, &t.name),
                kind: ToolKind::SandboxFile {
                    tool: t.name.clone(),
                },
                title: t.title.clone().or_else(|| t.annotations.title.clone()),
                description: format!(
                    "[sandbox files] {description} Paths are inside the sandbox \
                     ({WORKSPACE_DIR}); this doesn't reach the user's computer."
                ),
                schema: normalize_schema(&t.input_schema),
                read_only,
                destructive: !read_only && t.annotations.destructive_hint == Some(true),
            }
        })
        .collect()
}

fn text_arg<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The command a `sandbox_exec` call asks for, or why it can't run.
pub fn command_of(input: &Value) -> Result<String, String> {
    let c = text_arg(input, &["command", "cmd"])
        .ok_or_else(|| "sandbox_exec needs a \"command\" string.".to_owned())?;
    if c.chars().count() > MAX_COMMAND_CHARS {
        return Err(format!(
            "The command is too long (at most {MAX_COMMAND_CHARS} characters). Write a script \
             file with the sandbox's file tools and run that."
        ));
    }
    Ok(c.to_owned())
}

/// The label of a sandbox call in "Used in this session".
pub fn label(tool: &Builtin, input: &Value) -> Option<String> {
    match tool {
        Builtin::SandboxExec => command_of(input).ok().map(|c| command_label(&c)),
        Builtin::SandboxCopyIn | Builtin::SandboxCopyOut => {
            text_arg(input, &["from", "path", "source"]).map(str::to_owned)
        }
        _ => None,
    }
}

fn user_error(e: SandboxError) -> String {
    match e {
        SandboxError::DockerMissing | SandboxError::DockerStopped(_) => format!(
            "{e} (The sandbox needs Docker; tell the user. Settings → Sandbox shows Docker's \
             status.)"
        ),
        other => other.to_string(),
    }
}

/// Runs `sandbox_exec`. Returns the result text and whether the command failed.
pub async fn exec(
    sandboxes: &Sandboxes,
    conversation_id: &str,
    input: &Value,
    timeout: Duration,
) -> Result<(String, bool), String> {
    let command = command_of(input)?;
    let cwd = text_arg(input, &["cwd", "dir", "folder"]).unwrap_or("");
    let r = sandboxes
        .exec(conversation_id, &command, cwd, timeout)
        .await
        .map_err(user_error)?;
    let failed = r.timed_out || r.code != Some(0);
    Ok((format_exec(&command, &r, timeout), failed))
}

fn format_exec(command: &str, r: &ExecResult, timeout: Duration) -> String {
    let mut s = format!("$ {command}\n(in {} in the sandbox)\n", r.cwd);
    if r.timed_out {
        s.push_str(&format!(
            "The command was stopped after {:.0} s (the time limit, Settings → Sandbox), with \
             everything it started. Run shorter steps, or long jobs in the background with \
             output to a file you check later.\n",
            timeout.as_secs_f32()
        ));
    } else {
        let secs = r.elapsed.as_secs_f32();
        match r.code {
            Some(c) => s.push_str(&format!("Exit code {c} after {secs:.1} s\n")),
            None => s.push_str(&format!("Ended by a signal after {secs:.1} s\n")),
        }
    }
    if !r.stdout.trim().is_empty() {
        s.push_str("\nstdout:\n");
        s.push_str(&clip(r.stdout.trim_end()));
        s.push('\n');
    }
    if !r.stderr.trim().is_empty() {
        s.push_str("\nstderr:\n");
        s.push_str(&clip(r.stderr.trim_end()));
        s.push('\n');
    }
    if r.stdout.trim().is_empty() && r.stderr.trim().is_empty() && !r.timed_out {
        s.push_str("(no output)\n");
    }
    s
}

/// Keeps the start and the end of long output.
fn clip(text: &str) -> String {
    let n = text.chars().count();
    if n <= MAX_OUTPUT_CHARS {
        return text.to_owned();
    }
    let head: String = text.chars().take(MAX_OUTPUT_CHARS / 2).collect();
    let tail: String = text.chars().skip(n - MAX_OUTPUT_CHARS / 2).collect();
    format!(
        "{head}\n[… {} characters cut …]\n{tail}",
        n - MAX_OUTPUT_CHARS
    )
}

/// Runs `sandbox_copy_in`: `from` must be inside a shared folder.
pub async fn copy_in(
    sandboxes: &Sandboxes,
    shared: &Sandbox,
    conversation_id: &str,
    input: &Value,
) -> Result<String, String> {
    if shared.is_empty() {
        return Err(
            "No folder is shared with this chat, so there is nothing to copy in. Ask the user \
             to add one (the “+” menu → Add folder)."
                .into(),
        );
    }
    let from = text_arg(input, &["from", "path", "source"])
        .ok_or_else(|| "sandbox_copy_in needs \"from\": a path in a shared folder.".to_owned())?
        .to_owned();
    let to = text_arg(input, &["to", "dest", "destination"])
        .unwrap_or("")
        .to_owned();
    let sb = shared.clone();
    let host = tokio::task::spawn_blocking(move || sb.resolve(&from))
        .await
        .map_err(|e| e.to_string())??;
    if !host.exists() {
        return Err(format!("{} doesn't exist.", display_path(&host)));
    }
    let at = sandboxes
        .copy_in(conversation_id, &host, &to)
        .await
        .map_err(user_error)?;
    Ok(format!(
        "Copied {} into the sandbox as {at}.",
        display_path(&host)
    ))
}

/// Runs `sandbox_copy_out`: `to` must be inside a shared folder.
pub async fn copy_out(
    sandboxes: &Sandboxes,
    shared: &Sandbox,
    conversation_id: &str,
    input: &Value,
) -> Result<String, String> {
    if shared.is_empty() {
        return Err(
            "No folder is shared with this chat, so there is nowhere to copy to. Ask the user \
             to add one (the “+” menu → Add folder)."
                .into(),
        );
    }
    let from = text_arg(input, &["from", "path", "source"])
        .ok_or_else(|| "sandbox_copy_out needs \"from\": a path in the sandbox.".to_owned())?
        .to_owned();
    let to = text_arg(input, &["to", "dest", "destination"])
        .unwrap_or("")
        .to_owned();
    let sb = shared.clone();
    let host = tokio::task::spawn_blocking(move || sb.resolve(&to))
        .await
        .map_err(|e| e.to_string())??;
    let at = sandboxes
        .copy_out(conversation_id, &from, &host)
        .await
        .map_err(user_error)?;
    Ok(format!(
        "Copied {from} from the sandbox to {}.",
        display_path(&at)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_checked() {
        assert_eq!(
            command_of(&json!({"command": "  make test "})).as_deref(),
            Ok("make test")
        );
        assert!(command_of(&json!({})).is_err());
        assert!(command_of(&json!({"command": "x".repeat(10_001)})).is_err());
        assert_eq!(
            label(
                &Builtin::SandboxExec,
                &json!({"command": "pytest -q\nmore"})
            )
            .as_deref(),
            Some("pytest -q")
        );
        assert_eq!(
            label(&Builtin::SandboxCopyOut, &json!({"from": "dist"})).as_deref(),
            Some("dist")
        );
    }

    #[test]
    fn results_show_code_and_both_streams() {
        let r = ExecResult {
            cwd: "/workspace/app".into(),
            code: Some(1),
            stdout: "ok\n".into(),
            stderr: "FAILED (failures=1)\n".into(),
            timed_out: false,
            elapsed: Duration::from_millis(1500),
        };
        let s = format_exec("python3 -m unittest", &r, Duration::from_secs(600));
        assert!(s.contains("$ python3 -m unittest"));
        assert!(s.contains("(in /workspace/app in the sandbox)"));
        assert!(s.contains("Exit code 1 after 1.5 s"));
        assert!(s.contains("stdout:\nok"));
        assert!(s.contains("stderr:\nFAILED"));
        let t = ExecResult {
            timed_out: true,
            code: None,
            stdout: String::new(),
            stderr: String::new(),
            ..r
        };
        let s = format_exec("sleep 99", &t, Duration::from_secs(5));
        assert!(s.contains("stopped after 5 s"), "{s}");
        assert!(clip(&"a".repeat(40_000)).chars().count() < 40_000);
    }

    #[test]
    fn file_tools_are_namespaced_and_keep_their_hints() {
        let tool = |name: &str, ro: bool| McpTool {
            server: "sandbox".into(),
            name: name.into(),
            title: None,
            description: Some("Does it.".into()),
            input_schema: r#"{"type":"object"}"#.into(),
            annotations: oxide_core::McpToolAnnotations {
                read_only_hint: Some(ro),
                ..Default::default()
            },
        };
        let tools = vec![tool("write_file", false), tool("read_text_file", true)];
        let e = file_entries(&tools);
        assert_eq!(e[0].api_name, "sandbox__write_file");
        assert!(!e[0].read_only);
        assert!(e[1].read_only);
        assert_eq!(e[1].server(), Some("sandbox"));
        assert_eq!(e[1].grant_key(), "builtin/sandbox__read_text_file");
        assert!(e[0].description.contains("/workspace"));
    }

    #[tokio::test]
    async fn copies_need_a_shared_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let s = Sandboxes::with_docker(
            dir.path().into(),
            None,
            std::sync::Arc::new(oxide_sandbox::NoSink),
            Some(dir.path().join("no-docker")),
        );
        let none = Sandbox::default();
        let e = copy_in(&s, &none, "c", &json!({"from": "a"}))
            .await
            .expect_err("no folders");
        assert!(e.contains("No folder is shared"), "{e}");
        let e = copy_out(&s, &none, "c", &json!({"from": "a"}))
            .await
            .expect_err("no folders");
        assert!(e.contains("No folder is shared"), "{e}");
        let e = exec(&s, "c", &json!({"command": "ls"}), Duration::from_secs(1))
            .await
            .expect_err("no docker");
        assert!(
            e.contains("Docker isn't installed") && e.contains("tell the user"),
            "{e}"
        );
    }
}
