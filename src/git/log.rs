use std::io::{BufRead, BufReader, Read};
use std::process::{Child, ChildStdout, Stdio};

use anyhow::{Context, Result, bail};

use super::{Repo, command};

const FIELD_SEPARATOR: char = '\u{1f}';
const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct%x1f%s";

#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub time: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: Person,
    pub committer: Person,
    pub subject: String,
}

pub struct CommitStream {
    child: Child,
    reader: BufReader<ChildStdout>,
    finished: bool,
}

impl CommitStream {
    pub fn spawn(repo: &Repo) -> Result<Self> {
        let mut child = command(
            &repo.root,
            &["log", "--all", "--date-order", "-z", LOG_FORMAT],
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run git log")?;
        let stdout = child.stdout.take().context("git log has no stdout")?;
        Ok(CommitStream {
            child,
            reader: BufReader::new(stdout),
            finished: false,
        })
    }

    pub fn next_batch(&mut self, size: usize) -> Result<Vec<Commit>> {
        let mut commits = Vec::with_capacity(size);
        while !self.finished && commits.len() < size {
            let mut record = Vec::new();
            if self.reader.read_until(0, &mut record)? == 0 {
                self.finish()?;
                break;
            }
            if record.last() == Some(&0) {
                record.pop();
            }
            let text = String::from_utf8_lossy(&record);
            let text = text.trim_start_matches('\n');
            if text.is_empty() {
                continue;
            }
            commits.push(parse_commit(text)?);
        }
        Ok(commits)
    }

    fn finish(&mut self) -> Result<()> {
        self.finished = true;
        let status = self.child.wait()?;
        if status.success() {
            return Ok(());
        }
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            pipe.read_to_string(&mut stderr)?;
        }
        if stderr.contains("does not have any commits") {
            return Ok(());
        }
        bail!("git log failed: {}", stderr.trim())
    }
}

fn parse_commit(record: &str) -> Result<Commit> {
    let fields: Vec<&str> = record.splitn(9, FIELD_SEPARATOR).collect();
    let [
        hash,
        parents,
        author_name,
        author_email,
        author_time,
        committer_name,
        committer_email,
        committer_time,
        subject,
    ] = fields[..]
    else {
        bail!("unexpected git log record: {record:?}");
    };
    Ok(Commit {
        hash: hash.to_string(),
        parents: parents.split_whitespace().map(str::to_string).collect(),
        author: Person {
            name: author_name.to_string(),
            email: author_email.to_string(),
            time: author_time.parse().unwrap_or(0),
        },
        committer: Person {
            name: committer_name.to_string(),
            email: committer_email.to_string(),
            time: committer_time.parse().unwrap_or(0),
        },
        subject: subject.to_string(),
    })
}
