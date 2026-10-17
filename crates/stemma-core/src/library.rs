//! The library: one directory holding every paper, each its own git
//! repository.

use std::path::{Path, PathBuf};

use crate::error::{GitError, Result};
use crate::git::Git;

#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperEntry {
    /// The directory name, shown as the paper's title.
    pub name: String,
    pub path: PathBuf,
}

impl Library {
    /// Opens the library at `root`, creating the directory if needed.
    pub async fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        tokio::fs::create_dir_all(&root).await?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The papers in the library, sorted by name. A paper is a directory
    /// with its own repository; linked worktrees, whose `.git` is a file,
    /// belong to another paper and are skipped.
    pub async fn papers(&self) -> Result<Vec<PaperEntry>> {
        let mut papers = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !tokio::fs::metadata(path.join(".git"))
                .await
                .is_ok_and(|m| m.is_dir())
            {
                continue;
            }
            papers.push(PaperEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path,
            });
        }
        papers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(papers)
    }

    /// Creates a paper named `name` with a `main.typ` to start writing in.
    /// Nothing is committed; the first save is the writer's.
    pub async fn create(&self, name: &str) -> Result<Git> {
        let name = name.trim();
        let valid = !name.is_empty()
            && !name.starts_with('.')
            && !name
                .chars()
                .any(|c| matches!(c, '/' | '\\') || c.is_control());
        if !valid {
            return Err(GitError::InvalidPaperName(name.to_owned()));
        }
        let path = self.root.join(name);
        if tokio::fs::try_exists(&path).await? {
            return Err(GitError::PaperExists(name.to_owned()));
        }

        let git = Git::init(&path).await?;
        git.write("main.typ", &format!("= {name}\n\n")).await?;
        Ok(git)
    }
}
