//! Managed Python (Phase 12): the status the UI shows, the `run_python` tool's name and
//! limits, and how files a script wrote are told apart.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::artifact::ArtifactKind;

/// The `run_python` tool's name.
pub const RUN_PYTHON: &str = "run_python";

/// The Python Oxide installs (a `uv` version request: the newest 3.12.x uv knows).
pub const PYTHON_VERSION: &str = "3.12";

/// The packages every Oxide Python environment has (see `python/pyproject.toml`).
pub const PYTHON_PACKAGES: [&str; 4] = ["pandas", "numpy", "matplotlib", "openpyxl"];

/// Default time limit of one script, in seconds.
pub const DEFAULT_PYTHON_TIMEOUT_SECS: u32 = 120;
/// Longest time limit Settings accepts for one script, in seconds.
pub const MAX_PYTHON_TIMEOUT_SECS: u32 = 3_600;
/// Longest script a call may send, in characters.
pub const MAX_PYTHON_CODE_CHARS: usize = 200_000;

/// Most images one run hands back to the chat.
pub const MAX_RUN_IMAGES: usize = 6;
/// Most text files one run turns into artifacts.
pub const MAX_RUN_ARTIFACTS: usize = 4;
/// Largest image a run hands back, in bytes.
pub const MAX_RUN_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
/// Largest text file a run turns into an artifact, in bytes.
pub const MAX_RUN_ARTIFACT_BYTES: u64 = 200 * 1024;

/// The Python tool (Settings → Permissions). Every run asks for approval until the user
/// chooses "Allow always for this chat".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct PythonSettings {
    /// Models may run Python code (`run_python`).
    pub enabled: bool,
    /// Time limit of one script, in seconds.
    pub timeout_secs: u32,
}

impl Default for PythonSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: DEFAULT_PYTHON_TIMEOUT_SECS,
        }
    }
}

/// Where the managed environment stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PythonState {
    /// Not installed yet (or out of date: the packages changed with an update).
    Missing,
    /// Being installed or repaired.
    Installing,
    Ready,
    /// The last install failed; `PythonStatus::error` says why.
    Failed,
}

/// One step of setting up the environment, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PythonStep {
    /// Find Oxide's `uv`.
    Uv,
    /// Install CPython.
    Python,
    /// Create the virtual environment.
    Venv,
    /// Install the packages from the lock file.
    Packages,
    /// Import them once to check.
    Check,
}

impl PythonStep {
    pub const ALL: [PythonStep; 5] = [
        PythonStep::Uv,
        PythonStep::Python,
        PythonStep::Venv,
        PythonStep::Packages,
        PythonStep::Check,
    ];

    /// 1-based position.
    pub fn number(self) -> u8 {
        match self {
            PythonStep::Uv => 1,
            PythonStep::Python => 2,
            PythonStep::Venv => 3,
            PythonStep::Packages => 4,
            PythonStep::Check => 5,
        }
    }

    /// What the UI says while the step runs.
    pub fn label(self) -> &'static str {
        match self {
            PythonStep::Uv => "Finding uv",
            PythonStep::Python => "Installing Python",
            PythonStep::Venv => "Creating the environment",
            PythonStep::Packages => "Installing packages",
            PythonStep::Check => "Checking the packages",
        }
    }
}

/// An installed package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PythonPackage {
    pub name: String,
    pub version: String,
}

/// The managed Python environment, as Settings → Advanced shows it. Also sent as the
/// `pythonChanged` event while it installs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PythonStatus {
    pub state: PythonState,
    /// The step running (or that failed).
    pub step: Option<PythonStep>,
    /// What is happening now, in a few words.
    pub message: Option<String>,
    /// Why the last install failed.
    pub error: Option<String>,
    /// The latest lines `uv` printed (newest last).
    pub log: Vec<String>,
    /// Which `uv` is used: its path, or `None` when none was found.
    pub uv_path: Option<String>,
    pub uv_version: Option<String>,
    /// The Python's full version (3.12.x), once installed.
    pub python_version: Option<String>,
    pub packages: Vec<PythonPackage>,
    /// The folder with the environment (`<app data>/python`).
    pub folder: String,
    /// Space the environment takes, when known.
    pub disk_bytes: Option<u64>,
}

impl PythonStatus {
    pub fn missing(folder: impl Into<String>) -> Self {
        Self {
            state: PythonState::Missing,
            step: None,
            message: None,
            error: None,
            log: Vec::new(),
            uv_path: None,
            uv_version: None,
            python_version: None,
            packages: Vec::new(),
            folder: folder.into(),
            disk_bytes: None,
        }
    }
}

/// What a file a script wrote becomes in the chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFile {
    /// Shown as an image (`media_type`).
    Image { media_type: &'static str },
    /// Opens as an artifact.
    Artifact {
        kind: ArtifactKind,
        language: Option<&'static str>,
    },
    /// Only listed.
    Other,
}

/// Classifies a file by its name.
pub fn classify_output(name: &str) -> OutputFile {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => OutputFile::Image {
            media_type: "image/png",
        },
        "jpg" | "jpeg" => OutputFile::Image {
            media_type: "image/jpeg",
        },
        "gif" => OutputFile::Image {
            media_type: "image/gif",
        },
        "webp" => OutputFile::Image {
            media_type: "image/webp",
        },
        "svg" => OutputFile::Artifact {
            kind: ArtifactKind::Svg,
            language: None,
        },
        "html" | "htm" => OutputFile::Artifact {
            kind: ArtifactKind::Html,
            language: None,
        },
        "md" | "markdown" => OutputFile::Artifact {
            kind: ArtifactKind::Markdown,
            language: None,
        },
        "csv" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("csv"),
        },
        "tsv" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("tsv"),
        },
        "json" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("json"),
        },
        "txt" | "log" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("text"),
        },
        "py" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("python"),
        },
        "yaml" | "yml" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("yaml"),
        },
        "xml" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("xml"),
        },
        "sql" => OutputFile::Artifact {
            kind: ArtifactKind::Code,
            language: Some("sql"),
        },
        _ => OutputFile::Other,
    }
}

/// A short label for the usage log: the script's first non-empty line.
pub fn code_label(code: &str) -> String {
    let line = code
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut out: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_classified_by_extension() {
        assert_eq!(
            classify_output("Chart.PNG"),
            OutputFile::Image {
                media_type: "image/png"
            }
        );
        assert_eq!(
            classify_output("data.csv"),
            OutputFile::Artifact {
                kind: ArtifactKind::Code,
                language: Some("csv")
            }
        );
        assert_eq!(
            classify_output("plot.svg"),
            OutputFile::Artifact {
                kind: ArtifactKind::Svg,
                language: None
            }
        );
        assert_eq!(classify_output("book.xlsx"), OutputFile::Other);
        assert_eq!(classify_output("noext"), OutputFile::Other);
    }

    #[test]
    fn code_label_is_the_first_line() {
        assert_eq!(
            code_label("\n\n  import pandas as pd\nx=1"),
            "import pandas as pd"
        );
        assert_eq!(code_label(""), "");
        let long = "x".repeat(100);
        assert_eq!(code_label(&long).chars().count(), 81);
    }

    #[test]
    fn steps_are_numbered_in_order() {
        let n: Vec<u8> = PythonStep::ALL.iter().map(|s| s.number()).collect();
        assert_eq!(n, vec![1, 2, 3, 4, 5]);
    }
}
