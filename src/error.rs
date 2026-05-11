use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("podman error: {0}")]
    Podman(String),

    #[error("project not found: no .claude-sandbox.toml or .git ancestor of {0}")]
    ProjectNotFound(std::path::PathBuf),

    #[error("name collision: '{0}' is already used by container at {1}")]
    NameCollision(String, std::path::PathBuf),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
