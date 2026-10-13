//! Commit history.

use super::Git;
use crate::error::{GitError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub oid: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    /// Author time in seconds since the Unix epoch.
    pub time: i64,
    pub summary: String,
    pub body: String,
}

/// Fields are separated by ASCII unit separators; `-z` terminates records with NUL.
const FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1f%b";

impl Git {
    /// Returns up to `limit` commits reachable from HEAD, newest first.
    /// With `path`, only commits touching that file, following renames.
    pub async fn log(&self, path: Option<&str>, limit: usize) -> Result<Vec<Commit>> {
        if self
            .run(&["rev-parse", "-q", "--verify", "HEAD"])
            .await
            .is_err()
        {
            return Ok(Vec::new());
        }
        let limit = format!("-n{limit}");
        let mut args = vec!["log", "-z", FORMAT, &limit];
        if let Some(path) = path {
            args.extend(["--follow", "--", path]);
        }
        parse(&self.run(&args).await?)
    }
}

fn parse(out: &[u8]) -> Result<Vec<Commit>> {
    out.split(|&b| b == 0)
        .filter(|r| !r.is_empty())
        .map(|record| {
            let record = String::from_utf8_lossy(record);
            let bad = || GitError::Parse(format!("bad log record {record:?}"));
            let f: Vec<_> = record.splitn(7, '\x1f').collect();
            let [oid, parents, name, email, time, summary, body] = f[..] else {
                return Err(bad());
            };
            Ok(Commit {
                oid: oid.to_owned(),
                parents: parents.split_whitespace().map(str::to_owned).collect(),
                author_name: name.to_owned(),
                author_email: email.to_owned(),
                time: time.parse().map_err(|_| bad())?,
                summary: summary.to_owned(),
                body: body.trim_end().to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records() {
        let out = b"bbb\x1faaa\x1fAda\x1fada@x.org\x1f1700000000\x1fRevise abstract\x1fTighter.\n\nCloses #3\n\0\
                    aaa\x1f\x1fAda\x1fada@x.org\x1f1690000000\x1fStart paper\x1f\0";
        let log = parse(out).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].parents, ["aaa"]);
        assert_eq!(log[0].body, "Tighter.\n\nCloses #3");
        assert_eq!(log[1].parents, Vec::<String>::new());
        assert_eq!(log[1].time, 1690000000);
        assert_eq!(log[1].body, "");
    }
}
