use thiserror::Error;

/// Why the sandbox couldn't do something. Messages are written for users (and models).
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error(
        "Docker isn't installed (or Oxide can't find the docker program). Install Docker \
         Desktop, start it, and try again."
    )]
    DockerMissing,
    #[error("Docker isn't running: {0}")]
    DockerStopped(String),
    #[error("{0}")]
    Docker(String),
    #[error("{0}")]
    Invalid(String),
    #[error("The sandbox's file server failed: {0}")]
    Files(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}
