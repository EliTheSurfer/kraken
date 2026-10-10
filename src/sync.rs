use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::git::Repo;

#[derive(Clone, Debug, PartialEq)]
pub enum SyncEvent {
    Fetching,
    Fetched { error: Option<String> },
    Changed,
}

pub enum SyncCommand {
    FetchNow,
    Rebaseline,
}

pub fn fetch_all(repo: &Repo) -> Result<()> {
    let ssh = repo
        .run(&["config", "--get", "core.sshCommand"])
        .map(|command| command.trim().to_string())
        .unwrap_or_else(|_| "ssh".to_string());
    let output = crate::git::command(&repo.root, &["fetch", "--all", "--prune", "--quiet"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", format!("{ssh} -o BatchMode=yes"))
        .stdin(Stdio::null())
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr);
    bail!(
        "{}",
        message
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("git fetch failed")
    )
}

pub fn signature(repo: &Repo) -> String {
    let refs = repo
        .run(&["for-each-ref", "--format=%(refname) %(objectname)"])
        .unwrap_or_default();
    let head = repo.run(&["rev-parse", "HEAD"]).unwrap_or_default();
    let branch = repo
        .run(&["symbolic-ref", "-q", "HEAD"])
        .unwrap_or_default();
    format!("{refs}\n{head}{branch}")
}

pub fn fetch_due(last_fetch: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last_fetch.is_none_or(|last| now.duration_since(last) >= interval)
}

pub struct SyncWorker {
    pub commands: Sender<SyncCommand>,
    pub events: Receiver<SyncEvent>,
}

impl SyncWorker {
    pub fn start(repo_root: PathBuf, poll: Duration, fetch_interval: Duration) -> SyncWorker {
        let (commands, command_receiver) = channel();
        let (event_sender, events) = channel();
        thread::spawn(move || {
            let repo = Repo { root: repo_root };
            let mut baseline = signature(&repo);
            let mut last_fetch: Option<Instant> = None;
            loop {
                let fetch_now = match command_receiver.recv_timeout(poll) {
                    Ok(SyncCommand::FetchNow) => true,
                    Ok(SyncCommand::Rebaseline) => {
                        baseline = signature(&repo);
                        continue;
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
                if fetch_now || fetch_due(last_fetch, Instant::now(), fetch_interval) {
                    if event_sender.send(SyncEvent::Fetching).is_err() {
                        return;
                    }
                    let error = fetch_all(&repo).err().map(|error| format!("{error:#}"));
                    last_fetch = Some(Instant::now());
                    if event_sender.send(SyncEvent::Fetched { error }).is_err() {
                        return;
                    }
                }
                let current = signature(&repo);
                if current != baseline {
                    baseline = current;
                    if event_sender.send(SyncEvent::Changed).is_err() {
                        return;
                    }
                }
            }
        });
        SyncWorker { commands, events }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_is_due_at_start_and_after_the_interval() {
        let now = Instant::now();
        let minute = Duration::from_secs(60);
        assert!(fetch_due(None, now, minute));
        assert!(!fetch_due(Some(now - Duration::from_secs(30)), now, minute));
        assert!(fetch_due(Some(now - Duration::from_secs(61)), now, minute));
    }
}
