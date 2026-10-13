//! Word-level diffs and reading files at a revision.
//!
//! Prose is usually one long line per paragraph, so line diffs are useless
//! for papers. Everything here uses `--word-diff=porcelain` instead.

use super::Git;
use crate::error::{GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// Path before the change, or `None` if the file was added.
    pub old_path: Option<String>,
    /// Path after the change, or `None` if the file was deleted.
    pub new_path: Option<String>,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Same(String),
    Removed(String),
    Added(String),
    Newline,
}

impl Git {
    /// Word diff from `from` to `to`, or to the work tree when `to` is `None`.
    /// With `path`, only that file is compared.
    pub async fn word_diff(
        &self,
        from: &str,
        to: Option<&str>,
        path: Option<&str>,
    ) -> Result<Vec<FileDiff>> {
        let mut args = vec![
            "diff",
            "--word-diff=porcelain",
            "--no-ext-diff",
            "--end-of-options",
            from,
        ];
        args.extend(to);
        args.push("--");
        args.extend(path);
        parse(&String::from_utf8_lossy(&self.run(&args).await?))
    }

    /// Contents of `path` as of `rev`.
    pub async fn show(&self, rev: &str, path: &str) -> Result<String> {
        let out = self
            .run(&["show", "--end-of-options", &format!("{rev}:{path}")])
            .await?;
        String::from_utf8(out).map_err(|_| GitError::Parse(format!("{path} at {rev} is not UTF-8")))
    }
}

fn parse(out: &str) -> Result<Vec<FileDiff>> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut in_hunk = false;

    for line in out.lines() {
        if line.starts_with("diff --git ") {
            files.push(FileDiff {
                old_path: None,
                new_path: None,
                hunks: Vec::new(),
            });
            in_hunk = false;
            continue;
        }
        let bad = || GitError::Parse(format!("bad diff line {line:?}"));
        let file = files.last_mut().ok_or_else(bad)?;

        if let Some(header) = line.strip_prefix("@@ ") {
            file.hunks.push(parse_hunk_header(header).ok_or_else(bad)?);
            in_hunk = true;
        } else if !in_hunk {
            if let Some(p) = line.strip_prefix("--- ") {
                file.old_path = side_path(p, "a/");
            } else if let Some(p) = line.strip_prefix("+++ ") {
                file.new_path = side_path(p, "b/");
            }
        } else {
            let hunk = file.hunks.last_mut().ok_or_else(bad)?;
            let (marker, text) = line.split_at_checked(1).ok_or_else(bad)?;
            hunk.segments.push(match marker {
                " " => Segment::Same(text.to_owned()),
                "-" => Segment::Removed(text.to_owned()),
                "+" => Segment::Added(text.to_owned()),
                "~" => Segment::Newline,
                // "\ No newline at end of file"
                "\\" => continue,
                _ => return Err(bad()),
            });
        }
    }
    Ok(files)
}

fn side_path(path: &str, prefix: &str) -> Option<String> {
    path.strip_prefix(prefix).map(str::to_owned)
}

/// Parses `-1,3 +1,4 @@ ...` into a hunk with start lines.
fn parse_hunk_header(header: &str) -> Option<Hunk> {
    let mut parts = header.split(' ');
    let start = |s: &str| s.split(',').next()?.parse().ok();
    let old_start = start(parts.next()?.strip_prefix('-')?)?;
    let new_start = start(parts.next()?.strip_prefix('+')?)?;
    Some(Hunk {
        old_start,
        new_start,
        segments: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Segment::*;

    #[test]
    fn parses_word_diff() {
        let out = "\
diff --git a/main.typ b/main.typ
index 1111111..2222222 100644
--- a/main.typ
+++ b/main.typ
@@ -1,2 +1,2 @@
 We
-show
+prove
  that
~
 Done.
~
diff --git a/new.typ b/new.typ
new file mode 100644
--- /dev/null
+++ b/new.typ
@@ -0,0 +1 @@
+Hello
~
\\ No newline at end of file
";
        let files = parse(out).unwrap();
        assert_eq!(files.len(), 2);

        assert_eq!(files[0].old_path.as_deref(), Some("main.typ"));
        assert_eq!(files[0].hunks[0].old_start, 1);
        assert_eq!(
            files[0].hunks[0].segments,
            [
                Same("We".into()),
                Removed("show".into()),
                Added("prove".into()),
                Same(" that".into()),
                Newline,
                Same("Done.".into()),
                Newline,
            ]
        );

        assert_eq!(files[1].old_path, None);
        assert_eq!(files[1].new_path.as_deref(), Some("new.typ"));
        assert_eq!(files[1].hunks[0].new_start, 1);
        assert_eq!(files[1].hunks[0].segments, [Added("Hello".into()), Newline]);
    }

    #[test]
    fn no_changes_is_empty() {
        assert!(parse("").unwrap().is_empty());
    }
}
