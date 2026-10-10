mod support;

use std::time::{Duration, Instant};

use kraken::git::{Repo, read_refs};
use kraken::sync::{SyncCommand, SyncEvent, SyncWorker, fetch_all, signature};
use support::TestRepo;

fn clone_of(upstream: &TestRepo) -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("clone");
    let status = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(upstream.path())
        .arg(&path)
        .status()
        .expect("clone");
    assert!(status.success());
    (dir, Repo::discover(&path).expect("discover"))
}

fn wait_for(worker: &SyncWorker, wanted: impl Fn(&SyncEvent) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(event) = worker.events.recv_timeout(Duration::from_millis(200))
            && wanted(&event)
        {
            return true;
        }
    }
    false
}

#[test]
fn fetch_brings_a_branch_pushed_upstream() {
    let upstream = TestRepo::new();
    upstream.commit("base");
    let (_dir, clone) = clone_of(&upstream);
    upstream.branch("pushed-by-someone");
    fetch_all(&clone).expect("fetch");
    let refs = read_refs(&clone).expect("refs");
    assert!(
        refs.refs
            .iter()
            .any(|reference| reference.name == "origin/pushed-by-someone")
    );
}

#[test]
fn signature_changes_when_a_ref_moves() {
    let test_repo = TestRepo::new();
    test_repo.commit("base");
    let repo = Repo::discover(test_repo.path()).expect("discover");
    let before = signature(&repo);
    assert_eq!(before, signature(&repo));
    test_repo.commit("more");
    assert_ne!(before, signature(&repo));
}

#[test]
fn worker_reports_a_change_made_outside() {
    let test_repo = TestRepo::new();
    test_repo.commit("base");
    let worker = SyncWorker::start(
        test_repo.path().to_path_buf(),
        Duration::from_millis(100),
        Duration::from_secs(3600),
    );
    std::thread::sleep(Duration::from_millis(300));
    test_repo.branch("made-elsewhere");
    assert!(wait_for(&worker, |event| *event == SyncEvent::Changed));
}

#[test]
fn worker_fetches_on_demand_and_the_new_branch_is_reported() {
    let upstream = TestRepo::new();
    upstream.commit("base");
    let (_dir, clone) = clone_of(&upstream);
    let worker = SyncWorker::start(
        clone.root.clone(),
        Duration::from_millis(100),
        Duration::from_secs(3600),
    );
    assert!(wait_for(&worker, |event| matches!(
        event,
        SyncEvent::Fetched { error: None }
    )));
    upstream.branch("pushed-later");
    worker.commands.send(SyncCommand::FetchNow).expect("send");
    assert!(wait_for(&worker, |event| *event == SyncEvent::Changed));
}
