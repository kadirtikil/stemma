use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git executable not found")]
    NotInstalled,

    #[error("not a git repository: {0}")]
    NotARepo(PathBuf),

    #[error("git {args} exited with {code:?}: {stderr}")]
    Command {
        args: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("draft {branch} is already open in {}", path.display())]
    BranchCheckedOut { branch: String, path: PathBuf },

    #[error("\"{0}\" can't be used as a paper name")]
    InvalidPaperName(String),

    #[error("a paper named \"{0}\" already exists")]
    PaperExists(String),

    #[error("{0} is outside the paper")]
    OutsideRepo(String),

    #[error("failed to parse git output: {0}")]
    Parse(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GitError>;
