//! The shell tool: commands run in a chat's granted folders (Phase 11, off by default).
//!
//! Windows runs commands in PowerShell (`pwsh` when installed, else Windows PowerShell),
//! other systems in `sh`. Each command has a time limit; its output is captured and cut to
//! a size models can read. Whether a command needs approval is decided by the chat service
//! ([`oxide_core::workspace::command_allowed`]).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use oxide_core::workspace::RUN_COMMAND;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;

use super::files::{Sandbox, display_path};
use super::{Builtin, ToolEntry, ToolKind};

/// Most characters of stdout (and of stderr) a result shows.
pub const MAX_OUTPUT_CHARS: usize = 30_000;
/// Most bytes read from each stream (the rest is drained and dropped).
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;
/// Longest command, in characters.
const MAX_COMMAND_CHARS: usize = 10_000;

/// The shell commands run in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    pub program: String,
    pub args: Vec<String>,
    /// What models are told ("PowerShell 7", "Windows PowerShell", "sh").
    pub label: String,
}

impl Shell {
    /// The shell of this system.
    pub fn detect() -> Self {
        if cfg!(windows) {
            if which("pwsh.exe") {
                Self::powershell("pwsh.exe", "PowerShell 7")
            } else {
                Self::powershell("powershell.exe", "Windows PowerShell")
            }
        } else {
            Self {
                program: "sh".into(),
                args: vec!["-c".into()],
                label: "sh (POSIX shell)".into(),
            }
        }
    }

    fn powershell(program: &str, label: &str) -> Self {
        Self {
            program: program.into(),
            args: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-Command".into(),
            ],
            label: label.into(),
        }
    }

    fn is_powershell(&self) -> bool {
        self.program.to_ascii_lowercase().contains("powershell")
            || self.program.to_ascii_lowercase().starts_with("pwsh")
    }
}

fn which(exe: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(exe).is_file()))
        .unwrap_or(false)
}

/// The command a call asks for (trimmed), or why it can't run.
pub fn command_of(input: &Value) -> Result<String, String> {
    let c = input
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if c.is_empty() {
        return Err("run_command needs a \"command\" string.".into());
    }
    if c.chars().count() > MAX_COMMAND_CHARS {
        return Err(format!(
            "The command is too long (at most {MAX_COMMAND_CHARS} characters)."
        ));
    }
    Ok(c.to_owned())
}

/// Runs a command in a granted folder. `timeout` ends it (and its child processes).
pub async fn run_command(
    sandbox: &Sandbox,
    shell: &Shell,
    input: &Value,
    timeout: Duration,
) -> Result<(String, bool), String> {
    let command = command_of(input)?;
    let cwd_raw = input
        .get("cwd")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let sb = sandbox.clone();
    let cwd = tokio::task::spawn_blocking(move || sb.resolve(&cwd_raw))
        .await
        .map_err(|e| e.to_string())??;
    if !cwd.is_dir() {
        return Err(format!(
            "The working folder “{}” doesn't exist.",
            display_path(&cwd)
        ));
    }
    let script = if shell.is_powershell() {
        // UTF-8 output instead of the console's code page.
        format!(
            "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = \
             [System.Text.Encoding]::UTF8; {command}"
        )
    } else {
        command.clone()
    };
    let mut cmd = tokio::process::Command::new(&shell.program);
    cmd.args(&shell.args)
        .arg(&script)
        .current_dir(strip_verbatim(&cwd))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console window flashes up.
        cmd.creation_flags(0x0800_0000);
    }
    let started = std::time::Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{} couldn't start: {e}", shell.label))?;
    let pid = child.id();
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let read_out = async move { capture(out.as_mut()).await };
    let read_err = async move { capture(err.as_mut()).await };
    let waited = tokio::time::timeout(timeout, async {
        let (o, e, status) = tokio::join!(read_out, read_err, child.wait());
        (o, e, status)
    })
    .await;
    let secs = started.elapsed().as_secs_f32();
    match waited {
        Ok((stdout, stderr, status)) => {
            let status = status.map_err(|e| format!("The command failed: {e}"))?;
            let code = status.code();
            // PowerShell ends with 0 when only cmdlets failed (non-terminating errors, e.g.
            // a `Move-Item` whose file is gone): the errors on stderr still mean it failed.
            let errors =
                shell.is_powershell() && code == Some(0) && powershell_reported_errors(&stderr);
            let text = format_result(&command, &cwd, code, &stdout, &stderr, secs, false, errors);
            Ok((text, code != Some(0) || errors))
        }
        Err(_) => {
            kill_tree(pid).await;
            Ok((
                format_result(
                    &command,
                    &cwd,
                    None,
                    b"",
                    b"",
                    timeout.as_secs_f32(),
                    true,
                    false,
                ),
                true,
            ))
        }
    }
}

/// `current_dir` with `\\?\` paths can confuse PowerShell; use the plain form.
fn strip_verbatim(p: &Path) -> std::path::PathBuf {
    std::path::PathBuf::from(display_path(p))
}

async fn capture<R: tokio::io::AsyncRead + Unpin>(r: Option<&mut R>) -> Vec<u8> {
    let Some(r) = r else { return Vec::new() };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < MAX_CAPTURE_BYTES {
                    let take = n.min(MAX_CAPTURE_BYTES - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
            }
        }
    }
    buf
}

/// Ends a timed-out command and what it started.
async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    if cfg!(windows) {
        let mut k = tokio::process::Command::new("taskkill");
        k.args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            k.creation_flags(0x0800_0000);
        }
        let _ = k.status().await;
    } else {
        let _ = tokio::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
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

/// Whether PowerShell printed error records on stderr: Windows PowerShell's full view
/// (`CategoryInfo` / `FullyQualifiedErrorId`), or PowerShell 7's concise view, where a record
/// is a line starting with the command's name (`Move-Item: Cannot find path …`), sometimes
/// followed by a `Line |` block. Errors silenced with `-ErrorAction SilentlyContinue` aren't
/// printed, so they don't count.
fn powershell_reported_errors(stderr: &[u8]) -> bool {
    let err = String::from_utf8_lossy(stderr);
    err.contains("FullyQualifiedErrorId")
        || err.contains("+ CategoryInfo")
        || err.lines().any(|line| {
            let line = line.trim_end();
            line == "Line |" || is_concise_error_line(line)
        })
}

/// `Verb-Noun: message`, as PowerShell 7 writes an error record (`Move-Item: …`,
/// `Write-Error: …`), not a native program's own `name: message`.
fn is_concise_error_line(line: &str) -> bool {
    let Some((name, _)) = line.split_once(':') else {
        return false;
    };
    let Some((verb, noun)) = name.split_once('-') else {
        return false;
    };
    let word = |w: &str| {
        w.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && w.chars().all(|c| c.is_ascii_alphanumeric())
    };
    word(verb) && word(noun)
}

#[allow(clippy::too_many_arguments)]
fn format_result(
    command: &str,
    cwd: &Path,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    secs: f32,
    timed_out: bool,
    errors_reported: bool,
) -> String {
    let mut s = format!("$ {command}\n(in {})\n", display_path(cwd));
    if timed_out {
        s.push_str(&format!(
            "The command was stopped after {secs:.0} s (the time limit). Its output is lost; \
             run shorter steps or ask the user to raise the limit in Settings → Permissions.\n"
        ));
        return s;
    }
    match code {
        Some(c) => s.push_str(&format!("Exit code {c} after {secs:.1} s\n")),
        None => s.push_str(&format!("Ended by a signal after {secs:.1} s\n")),
    }
    if errors_reported {
        s.push_str(
            "PowerShell reported errors (see stderr), so this counts as failed: some steps may \
             not have run. Check the result before going on.\n",
        );
    }
    let out = String::from_utf8_lossy(stdout);
    let err = String::from_utf8_lossy(stderr);
    if !out.trim().is_empty() {
        s.push_str(&format!(
            "\n<stdout>\n{}\n</stdout>\n",
            clip(out.trim_end())
        ));
    }
    if !err.trim().is_empty() {
        s.push_str(&format!(
            "\n<stderr>\n{}\n</stderr>\n",
            clip(err.trim_end())
        ));
    }
    if out.trim().is_empty() && err.trim().is_empty() {
        s.push_str("(no output)\n");
    }
    s
}

/// The tool as models see it.
pub fn shell_entry(shell: &Shell) -> ToolEntry {
    ToolEntry {
        api_name: RUN_COMMAND.into(),
        kind: ToolKind::Builtin(Builtin::RunCommand),
        title: Some("Run command".into()),
        description: format!(
            "Run a command in {} on the user's computer, in a shared folder (`cwd`, default: \
             the first shared folder). Returns the exit code and output. Commands can't read \
             input and are stopped after a time limit. The user approves commands unless \
             they're on their allowlist.",
            shell.label
        ),
        schema: json!({"type": "object", "properties": {
            "command": {"type": "string", "description": format!("The command, in {} syntax.", shell.label)},
            "cwd": {"type": "string", "description": "Working folder inside a shared folder."}
        }, "required": ["command"]}),
        read_only: false,
        destructive: true,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use oxide_core::ChatFolder;

    fn sandbox(dir: &Path) -> Sandbox {
        Sandbox::from_folders(
            &[ChatFolder {
                path: dir.to_string_lossy().into_owned(),
                name: "work".into(),
                added_at: 0,
            }],
            false,
        )
    }

    #[tokio::test]
    async fn commands_run_in_the_folder_and_report_output() {
        let tmp = tempfile::tempdir().expect("tmp");
        std::fs::create_dir(tmp.path().join("sub")).expect("mkdir");
        let sb = sandbox(tmp.path());
        let sh = Shell::detect();
        let r = run_command(
            &sb,
            &sh,
            &json!({"command": "pwd; echo hi; echo oops >&2"}),
            Duration::from_secs(10),
        )
        .await
        .expect("runs");
        let (r, bad) = r;
        assert!(r.contains("Exit code 0"), "{r}");
        assert!(
            r.contains("<stdout>\n") && r.contains("hi\n</stdout>"),
            "{r}"
        );
        assert!(r.contains("<stderr>\noops\n</stderr>"), "{r}");
        assert!(!bad);
        let r = run_command(
            &sb,
            &sh,
            &json!({"command": "exit 3", "cwd": "sub"}),
            Duration::from_secs(10),
        )
        .await
        .expect("runs");
        assert!(
            r.0.contains("Exit code 3") && r.0.contains("sub"),
            "{}",
            r.0
        );
        assert!(r.1);
    }

    #[tokio::test]
    async fn time_limit_and_folder_checks() {
        let tmp = tempfile::tempdir().expect("tmp");
        let sb = sandbox(tmp.path());
        let sh = Shell::detect();
        let started = std::time::Instant::now();
        let r = run_command(
            &sb,
            &sh,
            &json!({"command": "sleep 20"}),
            Duration::from_millis(300),
        )
        .await
        .expect("runs");
        assert!(r.0.contains("time limit"), "{}", r.0);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(r.1);
        let e = run_command(
            &sb,
            &sh,
            &json!({"command": "ls", "cwd": "/"}),
            Duration::from_secs(5),
        )
        .await
        .expect_err("outside");
        assert!(e.contains("outside"), "{e}");
        assert!(
            run_command(
                &Sandbox::default(),
                &sh,
                &json!({"command": "ls"}),
                Duration::from_secs(5)
            )
            .await
            .is_err()
        );
        assert!(command_of(&json!({"command": "  "})).is_err());
    }

    #[test]
    fn long_output_keeps_both_ends() {
        let text = format!("{}{}", "a".repeat(MAX_OUTPUT_CHARS), "b".repeat(100));
        let c = clip(&text);
        assert!(c.starts_with('a') && c.ends_with('b') && c.contains("cut"));
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn powershell_error_records_count_as_failures() {
        let stderr = b"move : Cannot find path 'C:\\x' because it does not exist.\r\n\
            + CategoryInfo          : ObjectNotFound: (C:\\x:String) [Move-Item]\r\n\
            + FullyQualifiedErrorId : PathNotFound,Microsoft.PowerShell.Commands.MoveItemCommand";
        assert!(powershell_reported_errors(stderr));
        assert!(!powershell_reported_errors(
            b"warning: something native said"
        ));
        // PowerShell 7's concise view (found on GitHub's Windows runner, 2026-11-22).
        assert!(powershell_reported_errors(
            b"Move-Item: Cannot find path 'C:\\x\\nope.txt' because it does not exist.\r\n"
        ));
        assert!(powershell_reported_errors(
            b"Get-Content:\r\nLine |\r\n   1 |  Get-Content nope\r\n     | Cannot find path.\r\n"
        ));
        // Native programs' own messages don't count.
        let natives: [&[u8]; 4] = [
            b"npm-cli: warn deprecated",
            b"error: could not compile",
            b"git-lfs: not installed",
            b"x-y: z",
        ];
        for native in natives {
            assert!(
                !powershell_reported_errors(native),
                "{}",
                String::from_utf8_lossy(native)
            );
        }
        let text = format_result(
            "move a b",
            Path::new("/tmp"),
            Some(0),
            b"",
            stderr,
            1.0,
            false,
            true,
        );
        assert!(text.contains("Exit code 0"), "{text}");
        assert!(text.contains("counts as failed"), "{text}");
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use oxide_core::ChatFolder;

    #[tokio::test]
    async fn powershell_runs_in_the_folder() {
        let tmp = tempfile::tempdir().expect("tmp");
        let sb = Sandbox::from_folders(
            &[ChatFolder {
                path: tmp.path().to_string_lossy().into_owned(),
                name: "work".into(),
                added_at: 0,
            }],
            false,
        );
        let sh = Shell::detect();
        assert!(sh.label.contains("PowerShell"), "{}", sh.label);
        let (out, failed) = run_command(
            &sb,
            &sh,
            &json!({"command": "Write-Output 'hej æøå'; (Get-Location).Path"}),
            Duration::from_secs(60),
        )
        .await
        .expect("runs");
        assert!(!failed, "{out}");
        assert!(out.contains("hej æøå"), "{out}");
        let (out, failed) = run_command(
            &sb,
            &sh,
            &json!({"command": "exit 4"}),
            Duration::from_secs(60),
        )
        .await
        .expect("runs");
        assert!(failed && out.contains("Exit code 4"), "{out}");
        // A cmdlet fails, then a later line works: PowerShell says 0, Oxide says failed.
        let (out, failed) = run_command(
            &sb,
            &sh,
            &json!({"command": "Move-Item nope.txt elsewhere.txt\nWrite-Output done"}),
            Duration::from_secs(60),
        )
        .await
        .expect("runs");
        assert!(
            failed && out.contains("Exit code 0") && out.contains("counts as failed"),
            "{out}"
        );
        // Silenced errors don't count.
        let (out, failed) = run_command(
            &sb,
            &sh,
            &json!({
                "command": "Move-Item nope.txt elsewhere.txt -ErrorAction SilentlyContinue\nWrite-Output done"
            }),
            Duration::from_secs(60),
        )
        .await
        .expect("runs");
        assert!(!failed, "{out}");
    }
}
