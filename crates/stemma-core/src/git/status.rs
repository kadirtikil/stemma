//! `git status --porcelain=v2 -z --branch`.

use super::Git;
use crate::error::{GitError, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// Current branch, or `None` when HEAD is detached.
    pub branch: Option<String>,
    /// Commit HEAD points at, or `None` before the first commit.
    pub head: Option<String>,
    pub upstream: Option<String>,
    /// Commits (ahead, behind) relative to upstream.
    pub ahead_behind: Option<(u32, u32)>,
    pub files: Vec<FileStatus>,
}

impl Status {
    pub fn is_clean(&self) -> bool {
        self.files.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStatus {
    pub path: String,
    /// Previous path, for renames and copies.
    pub orig_path: Option<String>,
    /// Change between HEAD and the index.
    pub staged: Change,
    /// Change between the index and the work tree.
    pub unstaged: Change,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Unmodified,
    Modified,
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Conflicted,
}

impl Change {
    fn from_code(c: u8) -> Result<Self> {
        Ok(match c {
            b'.' => Self::Unmodified,
            b'M' => Self::Modified,
            b'T' => Self::TypeChanged,
            b'A' => Self::Added,
            b'D' => Self::Deleted,
            b'R' => Self::Renamed,
            b'C' => Self::Copied,
            _ => {
                return Err(GitError::Parse(format!(
                    "unknown status code {:?}",
                    c as char
                )));
            }
        })
    }
}

impl Git {
    pub async fn status(&self) -> Result<Status> {
        let out = self
            .run(&[
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=all",
            ])
            .await?;
        parse(&out)
    }
}

fn parse(out: &[u8]) -> Result<Status> {
    let mut status = Status::default();
    let mut entries = out
        .split(|&b| b == 0)
        .filter(|e| !e.is_empty())
        .map(String::from_utf8_lossy);

    while let Some(entry) = entries.next() {
        let bad = || GitError::Parse(format!("bad status entry {entry:?}"));
        let (kind, rest) = entry.split_once(' ').ok_or_else(bad)?;
        match kind {
            "#" => parse_header(&mut status, rest)?,
            "1" => {
                let f: Vec<_> = rest.splitn(8, ' ').collect();
                let [xy, .., path] = f[..] else {
                    return Err(bad());
                };
                status.files.push(tracked(xy, path, None)?);
            }
            "2" => {
                let f: Vec<_> = rest.splitn(9, ' ').collect();
                let [xy, .., path] = f[..] else {
                    return Err(bad());
                };
                let orig = entries.next().ok_or_else(bad)?.into_owned();
                status.files.push(tracked(xy, path, Some(orig))?);
            }
            "u" => {
                let f: Vec<_> = rest.splitn(10, ' ').collect();
                let [_, .., path] = f[..] else {
                    return Err(bad());
                };
                status.files.push(FileStatus {
                    path: path.to_owned(),
                    orig_path: None,
                    staged: Change::Conflicted,
                    unstaged: Change::Conflicted,
                });
            }
            "?" => status.files.push(FileStatus {
                path: rest.to_owned(),
                orig_path: None,
                staged: Change::Unmodified,
                unstaged: Change::Untracked,
            }),
            // Ignored files are only listed with --ignored, which we don't pass.
            "!" => {}
            _ => return Err(bad()),
        }
    }
    Ok(status)
}

fn parse_header(status: &mut Status, header: &str) -> Result<()> {
    let Some((key, value)) = header.split_once(' ') else {
        return Ok(());
    };
    match key {
        "branch.oid" if value != "(initial)" => status.head = Some(value.to_owned()),
        "branch.head" if value != "(detached)" => status.branch = Some(value.to_owned()),
        "branch.upstream" => status.upstream = Some(value.to_owned()),
        "branch.ab" => {
            let parse = |s: Option<&str>, sign| {
                s.and_then(|s| s.strip_prefix(sign))
                    .and_then(|n| n.parse().ok())
                    .ok_or_else(|| GitError::Parse(format!("bad branch.ab {value:?}")))
            };
            let mut parts = value.split(' ');
            let ahead = parse(parts.next(), '+')?;
            let behind = parse(parts.next(), '-')?;
            status.ahead_behind = Some((ahead, behind));
        }
        _ => {}
    }
    Ok(())
}

fn tracked(xy: &str, path: &str, orig_path: Option<String>) -> Result<FileStatus> {
    let &[x, y] = xy.as_bytes() else {
        return Err(GitError::Parse(format!("bad XY {xy:?}")));
    };
    Ok(FileStatus {
        path: path.to_owned(),
        orig_path,
        staged: Change::from_code(x)?,
        unstaged: Change::from_code(y)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headers_and_entries() {
        let out = b"# branch.oid 1234abcd\0\
                    # branch.head main\0\
                    # branch.upstream origin/main\0\
                    # branch.ab +2 -1\0\
                    1 .M N... 100644 100644 100644 aaa bbb paper with spaces.typ\0\
                    2 R. N... 100644 100644 100644 aaa bbb R100 intro.typ\0chapters/intro.typ\0\
                    u UU N... 100644 100644 100644 100644 aaa bbb ccc refs.bib\0\
                    ? figures/plot \xc3\xa9.svg\0";
        let s = parse(out).unwrap();

        assert_eq!(s.head.as_deref(), Some("1234abcd"));
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!(s.ahead_behind, Some((2, 1)));
        assert_eq!(
            s.files,
            [
                FileStatus {
                    path: "paper with spaces.typ".into(),
                    orig_path: None,
                    staged: Change::Unmodified,
                    unstaged: Change::Modified,
                },
                FileStatus {
                    path: "intro.typ".into(),
                    orig_path: Some("chapters/intro.typ".into()),
                    staged: Change::Renamed,
                    unstaged: Change::Unmodified,
                },
                FileStatus {
                    path: "refs.bib".into(),
                    orig_path: None,
                    staged: Change::Conflicted,
                    unstaged: Change::Conflicted,
                },
                FileStatus {
                    path: "figures/plot é.svg".into(),
                    orig_path: None,
                    staged: Change::Unmodified,
                    unstaged: Change::Untracked,
                },
            ]
        );
    }

    #[test]
    fn initial_and_detached_are_none() {
        let s = parse(b"# branch.oid (initial)\0# branch.head (detached)\0").unwrap();
        assert_eq!(s, Status::default());
        assert!(s.is_clean());
    }
}
