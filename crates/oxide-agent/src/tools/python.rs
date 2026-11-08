//! The `run_python` tool (Phase 12): runs a script in Oxide's managed Python environment.
//!
//! The script runs in a folder: the chat's first granted folder (or `cwd` inside one), else a
//! private folder per chat in the app's data folder. What it writes there comes back with the
//! result: images are shown in the chat, small text files (csv, json, md, svg, html, …) open
//! as artifacts, anything else is listed. The code is *not* sandboxed (it runs as the user),
//! so every run asks for approval until the user chooses "Allow always for this chat".

use std::path::PathBuf;
use std::time::Duration;

use base64::Engine as _;
use oxide_core::artifact::{clean_key, clean_title};
use oxide_core::python::{
    MAX_PYTHON_CODE_CHARS, MAX_RUN_ARTIFACT_BYTES, MAX_RUN_ARTIFACTS, MAX_RUN_IMAGE_BYTES,
    MAX_RUN_IMAGES, OutputFile, classify_output,
};
use oxide_core::{ContentBlock, PYTHON_PACKAGES, PYTHON_VERSION, RUN_PYTHON};
use oxide_pyenv::{ChangedFile, PythonEnv, RunOutput, RunRequest, Snapshot};
use oxide_store::{NewArtifactVersion, Store};
use serde_json::{Value, json};

use super::files::{Sandbox, display_path, human_size};
use super::{Builtin, ToolEntry, ToolKind, ToolOutput, Tools};

/// Most characters of stdout (and of stderr) a result shows.
pub const MAX_OUTPUT_CHARS: usize = 30_000;

/// The tool as models see it.
pub fn python_entry() -> ToolEntry {
    ToolEntry {
        api_name: RUN_PYTHON.into(),
        kind: ToolKind::Builtin(Builtin::RunPython),
        title: Some("Run Python".into()),
        description: format!(
            "Run Python {PYTHON_VERSION} code on the user's computer and get its output. \
             {} are installed; other packages can't be installed and there is no internet \
             install. Print what you want to see: only stdout and stderr come back, not the value \
             of the last expression. The code runs in a folder (the first shared folder, or `cwd` \
             inside one; without shared folders, a private folder of this chat that keeps its files \
             between runs). Files the code writes there come back: images are shown to the user, \
             small text files (csv, json, md, svg, html) open as artifacts, other files are \
             listed. matplotlib draws to files: call plt.savefig('chart.png') (figures left open \
             are saved as figure-1.png, … when you save no image). The code isn't sandboxed and \
             the user approves each run; it is stopped after a time limit. The very first run \
             downloads Python and the packages, which takes a few minutes.",
            PYTHON_PACKAGES.join(", ")
        ),
        schema: json!({"type": "object", "properties": {
            "code": {"type": "string", "description": "The complete Python script."},
            "cwd": {"type": "string",
                    "description": "Working folder inside a shared folder (default: the first shared folder)."}
        }, "required": ["code"]}),
        read_only: false,
        destructive: true,
    }
}

/// The script a call asks for, or why it can't run.
pub fn code_of(input: &Value) -> Result<String, String> {
    let code = input.get("code").and_then(Value::as_str).unwrap_or("");
    if code.trim().is_empty() {
        return Err("run_python needs a \"code\" string.".into());
    }
    if code.chars().count() > MAX_PYTHON_CODE_CHARS {
        return Err(format!(
            "The code is too long (at most {MAX_PYTHON_CODE_CHARS} characters). Split it into \
             steps, or write it to a file in the shared folder and run that."
        ));
    }
    Ok(code.to_owned())
}

/// Where the script runs, and whether that is a folder the user shared.
async fn working_folder(
    env: &PythonEnv,
    chat_id: &str,
    sandbox: &Sandbox,
    raw: &str,
) -> Result<(PathBuf, bool), String> {
    if sandbox.is_empty() {
        if !raw.trim().is_empty() {
            return Err(
                "No folder is shared with this chat, so \"cwd\" can't be used: the code runs in \
                 the chat's own folder. Leave \"cwd\" out."
                    .into(),
            );
        }
        return Ok((env.work_dir(chat_id), false));
    }
    let (sb, raw) = (sandbox.clone(), raw.to_owned());
    let resolved = tokio::task::spawn_blocking(move || sb.resolve(&raw))
        .await
        .map_err(|e| e.to_string())??;
    if !resolved.is_dir() {
        return Err(format!(
            "The working folder “{}” doesn't exist.",
            display_path(&resolved)
        ));
    }
    // `\\?\` paths can confuse programs on Windows; use the plain form.
    Ok((PathBuf::from(display_path(&resolved)), true))
}

/// Runs one `run_python` call for chat `chat_id`, made by reply `reply_id`.
pub async fn run_python(
    tools: &Tools,
    store: &Store,
    chat_id: &str,
    reply_id: &str,
    sandbox: &Sandbox,
    input: &Value,
    timeout: Duration,
) -> ToolOutput {
    let Some(env) = tools.python() else {
        return ToolOutput::error("Python isn't available in this build.");
    };
    let code = match code_of(input) {
        Ok(c) => c,
        Err(e) => return ToolOutput::error(e),
    };
    let raw_cwd = input.get("cwd").and_then(Value::as_str).unwrap_or("");
    let (cwd, shared) = match working_folder(env, chat_id, sandbox, raw_cwd).await {
        Ok(f) => f,
        Err(e) => return ToolOutput::error(e),
    };

    let before = {
        let cwd = cwd.clone();
        tokio::task::spawn_blocking(move || Snapshot::take(&cwd))
            .await
            .unwrap_or_default()
    };
    let output = match env
        .run(RunRequest {
            code,
            cwd: cwd.clone(),
            timeout,
        })
        .await
    {
        Ok(o) => o,
        Err(e) => return ToolOutput::error(e.to_string()),
    };
    let changed = {
        let cwd = cwd.clone();
        tokio::task::spawn_blocking(move || {
            let after = Snapshot::take(&cwd);
            before.changed_since(&cwd, &after)
        })
        .await
        .unwrap_or_default()
    };

    let mut text = format_run(&output, &cwd, shared);
    let (blocks, listing) = attach_files(tools, store, chat_id, reply_id, &changed).await;
    text.push_str(&listing);
    let mut content = vec![ContentBlock::text(text)];
    content.extend(blocks);
    ToolOutput {
        content,
        is_error: output.timed_out || output.exit_code != Some(0),
        duration_ms: 0,
    }
}

/// Images become blocks and small text files artifacts; returns them with the text that
/// lists every changed file.
async fn attach_files(
    tools: &Tools,
    store: &Store,
    chat_id: &str,
    reply_id: &str,
    changed: &[ChangedFile],
) -> (Vec<ContentBlock>, String) {
    if changed.is_empty() {
        return (Vec::new(), String::new());
    }
    let mut blocks = Vec::new();
    let mut lines = Vec::new();
    let (mut images, mut artifacts) = (0usize, 0usize);
    for f in changed.iter().take(60) {
        let size = human_size(f.size);
        let name = f.rel.rsplit('/').next().unwrap_or(&f.rel).to_owned();
        let fate = match classify_output(&name) {
            OutputFile::Image { media_type }
                if images < MAX_RUN_IMAGES && f.size > 0 && f.size <= MAX_RUN_IMAGE_BYTES =>
            {
                match tokio::fs::read(&f.path).await {
                    Ok(bytes) => {
                        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                        let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
                        blocks.push(tools.image_block(media_type, data, stem).await);
                        images += 1;
                        "shown to the user".to_owned()
                    }
                    Err(e) => format!("couldn't be read ({e})"),
                }
            }
            OutputFile::Artifact { kind, language }
                if artifacts < MAX_RUN_ARTIFACTS
                    && f.size > 0
                    && f.size <= MAX_RUN_ARTIFACT_BYTES =>
            {
                match tokio::fs::read_to_string(&f.path).await {
                    Ok(content) if !content.trim().is_empty() => {
                        let key = clean_key(&format!("file {}", f.rel))
                            .unwrap_or_else(|| "file".to_owned());
                        let title = clean_title(&f.rel).unwrap_or_else(|| name.clone());
                        let saved = store
                            .save_artifact_version(NewArtifactVersion {
                                conversation_id: chat_id.to_owned(),
                                key,
                                kind,
                                title: title.clone(),
                                language: language.map(str::to_owned),
                                content,
                                message_id: Some(reply_id.to_owned()),
                            })
                            .await;
                        match saved {
                            Ok((artifact, _)) => {
                                artifacts += 1;
                                blocks.push(ContentBlock::ArtifactRef {
                                    artifact_id: artifact.id,
                                    title: artifact.title,
                                    version: artifact.version,
                                });
                                format!("opened as the artifact “{title}”")
                            }
                            Err(e) => format!("couldn't become an artifact ({e})"),
                        }
                    }
                    _ => "saved in the folder".to_owned(),
                }
            }
            _ => "saved in the folder".to_owned(),
        };
        lines.push(format!("- {} ({size}): {fate}", f.rel));
    }
    if changed.len() > 60 {
        lines.push(format!("- … and {} more", changed.len() - 60));
    }
    (
        blocks,
        format!("\nFiles written or changed:\n{}\n", lines.join("\n")),
    )
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

fn format_run(out: &RunOutput, cwd: &std::path::Path, shared: bool) -> String {
    let place = if shared {
        display_path(cwd)
    } else {
        format!("{} (this chat's own folder)", display_path(cwd))
    };
    if out.timed_out {
        return format!(
            "(in {place})\nThe script was stopped after {:.0} s (the time limit). Its output is \
             lost; run shorter steps or ask the user to raise the limit in Settings → \
             Permissions.\n",
            out.secs
        );
    }
    let mut s = match out.exit_code {
        Some(c) => format!("Exit code {c} after {:.1} s (in {place})\n", out.secs),
        None => format!("Ended by a signal after {:.1} s (in {place})\n", out.secs),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stdout.trim().is_empty() {
        s.push_str(&format!(
            "\n<stdout>\n{}\n</stdout>\n",
            clip(stdout.trim_end())
        ));
    }
    if !stderr.trim().is_empty() {
        s.push_str(&format!(
            "\n<stderr>\n{}\n</stderr>\n",
            clip(stderr.trim_end())
        ));
    }
    if stdout.trim().is_empty() && stderr.trim().is_empty() {
        s.push_str("(no output)\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_required_and_bounded() {
        assert!(code_of(&json!({})).is_err());
        assert!(code_of(&json!({"code": "  \n"})).is_err());
        assert_eq!(
            code_of(&json!({"code": "print(1)"})).as_deref(),
            Ok("print(1)")
        );
        let long = "x".repeat(MAX_PYTHON_CODE_CHARS + 1);
        assert!(code_of(&json!({"code": long})).is_err());
    }

    #[test]
    fn long_output_keeps_both_ends() {
        let text = format!("{}{}", "a".repeat(MAX_OUTPUT_CHARS), "b".repeat(100));
        let c = clip(&text);
        assert!(c.starts_with('a') && c.ends_with('b') && c.contains("cut"));
    }

    #[test]
    fn the_entry_names_the_packages() {
        let e = python_entry();
        assert_eq!(e.api_name, "run_python");
        assert!(e.description.contains("pandas") && e.description.contains("openpyxl"));
        assert!(e.destructive && !e.read_only);
    }
}
