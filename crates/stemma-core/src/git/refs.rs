//! Branches (drafts) and annotated tags (submitted versions).

use super::Git;
use crate::error::{GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// The commit the tag points at.
    pub target: String,
    /// First line of the tag message; empty for lightweight tags.
    pub subject: String,
}

impl Git {
    /// Local branches, sorted by name.
    pub async fn branches(&self) -> Result<Vec<Branch>> {
        let out = self
            .run(&[
                "for-each-ref",
                "--format=%(HEAD)%1f%(refname:short)",
                "refs/heads",
            ])
            .await?;
        records(&out, |f| {
            let [head, name] = f else { return None };
            Some(Branch {
                name: name.to_string(),
                is_current: *head == "*",
            })
        })
    }

    /// Creates branch `name` at HEAD and switches to it.
    pub async fn create_branch(&self, name: &str) -> Result<()> {
        check_name(name)?;
        self.run(&["switch", "-q", "-c", name]).await.map(drop)
    }

    pub async fn switch(&self, name: &str) -> Result<()> {
        check_name(name)?;
        self.run(&["switch", "-q", name]).await.map(drop)
    }

    /// Creates an annotated tag at HEAD.
    pub async fn tag(&self, name: &str, message: &str) -> Result<()> {
        check_name(name)?;
        self.run(&["tag", "-a", "-m", message, name])
            .await
            .map(drop)
    }

    /// All tags, oldest first.
    pub async fn tags(&self) -> Result<Vec<Tag>> {
        let out = self
            .run(&[
                "for-each-ref",
                "--sort=creatordate",
                "--format=%(refname:short)%1f%(if)%(*objectname)%(then)%(*objectname)%(else)%(objectname)%(end)%1f%(contents:subject)",
                "refs/tags",
            ])
            .await?;
        records(&out, |f| {
            let [name, target, subject] = f else {
                return None;
            };
            Some(Tag {
                name: name.to_string(),
                target: target.to_string(),
                subject: subject.to_string(),
            })
        })
    }
}

/// Git validates ref names itself, but a leading dash would be parsed as a flag.
pub(super) fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('-') {
        return Err(GitError::Parse(format!("invalid ref name {name:?}")));
    }
    Ok(())
}

/// Splits newline-separated records into unit-separated fields.
fn records<T>(out: &[u8], parse: impl Fn(&[&str]) -> Option<T>) -> Result<Vec<T>> {
    String::from_utf8_lossy(out)
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split('\x1f').collect();
            parse(&fields).ok_or_else(|| GitError::Parse(format!("bad ref record {line:?}")))
        })
        .collect()
}
