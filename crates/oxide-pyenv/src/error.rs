//! Errors of the managed Python environment. Messages are written for the user.

/// What can go wrong setting up or using the environment.
#[derive(Debug, thiserror::Error)]
pub enum PythonError {
    /// No `uv` binary: the sidecar isn't bundled (a development build that skipped
    /// `npm run fetch-uv`) and there is none on the PATH.
    #[error(
        "Oxide's uv wasn't found. In a development build, run `npm run fetch-uv` and rebuild; \
         otherwise install uv (https://docs.astral.sh/uv/) so it is on your PATH."
    )]
    NoUv,
    /// A setup step failed. `detail` has the last lines `uv` printed.
    #[error("{step} failed. {detail}")]
    Setup { step: String, detail: String },
    /// Repair or reset while a script is running.
    #[error("A Python script is still running. Try again when it has finished.")]
    Busy,
    /// A file or folder couldn't be read or written.
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    /// The Python process couldn't be started.
    #[error("Python couldn't be started: {0}")]
    Spawn(String),
}

impl PythonError {
    pub(crate) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        PythonError::Io {
            context: context.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, PythonError>;
