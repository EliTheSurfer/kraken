mod support;

use kraken::git::{CheckoutPlan, Head, Repo, commits_dropped_by_reset, read_refs, run_checkout};
use support::TestRepo;

struct Clone {
    _dir: tempfile::TempDir,
    repo: Repo,
}

fn clone_of(upstream: &TestRepo) -> Clone {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("clone");
    let status = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(upstream.path())
        .arg(&path)
        .status()
        .expect("clone");
    assert!(status.success());
    for (key, value) in [
        ("user.name", "Ada"),
        ("user.email", "ada@example.com"),
        ("commit.gpgsign", "false"),
    ] {
        Repo { root: path.clone() }
            .run(&["config", key, value])
            .expect("config");
    }
    Clone {
        _dir: dir,
        repo: Repo::discover(&path).expect("discover"),
    }
}

fn head(repo: &Repo) -> Head {
    read_refs(repo).expect("refs").head
}

fn upstream_with_feature() -> TestRepo {
    let upstream = TestRepo::new();
    upstream.commit("base");
    upstream.branch("feature");
    upstream.checkout("feature");
    upstream.commit("feature work");
    upstream.checkout("main");
    upstream
}

#[test]
fn switching_to_a_local_branch_moves_head() {
    let upstream = upstream_with_feature();
    upstream.branch("other");
    let repo = Repo::discover(upstream.path()).expect("discover");
    run_checkout(
        &repo,
        &CheckoutPlan::Switch {
            local: "other".to_string(),
        },
    )
    .expect("switch");
    assert_eq!(head(&repo), Head::Branch("other".to_string()));
}

#[test]
fn tracking_a_remote_branch_creates_the_local_branch() {
    let clone = clone_of(&upstream_with_feature());
    let plan = CheckoutPlan::Track {
        local: "feature".to_string(),
        remote: "origin/feature".to_string(),
    };
    run_checkout(&clone.repo, &plan).expect("track");
    assert_eq!(head(&clone.repo), Head::Branch("feature".to_string()));
    let upstream = clone
        .repo
        .run(&["rev-parse", "--abbrev-ref", "feature@{upstream}"])
        .expect("upstream");
    assert_eq!(upstream.trim(), "origin/feature");
}

#[test]
fn reset_counts_and_drops_local_commits_but_keeps_uncommitted_changes() {
    let clone = clone_of(&upstream_with_feature());
    clone
        .repo
        .run(&["switch", "-q", "feature"])
        .expect("switch");
    std::fs::write(clone.repo.root.join("local.txt"), "local").expect("write");
    clone.repo.run(&["add", "local.txt"]).expect("add");
    clone
        .repo
        .run(&["commit", "-q", "-m", "local only"])
        .expect("commit");
    std::fs::write(clone.repo.root.join("draft.txt"), "draft").expect("write draft");
    assert_eq!(
        commits_dropped_by_reset(&clone.repo, "feature", "origin/feature").expect("count"),
        1
    );
    let plan = CheckoutPlan::Reset {
        local: "feature".to_string(),
        remote: "origin/feature".to_string(),
        switch_first: false,
    };
    run_checkout(&clone.repo, &plan).expect("reset");
    let local = clone.repo.run(&["rev-parse", "feature"]).expect("local");
    let remote = clone
        .repo
        .run(&["rev-parse", "origin/feature"])
        .expect("remote");
    assert_eq!(local, remote);
    assert!(clone.repo.root.join("draft.txt").exists());
}

#[test]
fn reset_switches_to_the_local_branch_first_when_asked() {
    let clone = clone_of(&upstream_with_feature());
    clone
        .repo
        .run(&["branch", "-q", "feature", "origin/feature"])
        .expect("branch");
    let plan = CheckoutPlan::Reset {
        local: "feature".to_string(),
        remote: "origin/feature".to_string(),
        switch_first: true,
    };
    run_checkout(&clone.repo, &plan).expect("reset");
    assert_eq!(head(&clone.repo), Head::Branch("feature".to_string()));
}

#[test]
fn a_switch_that_would_lose_uncommitted_changes_is_refused() {
    let upstream = upstream_with_feature();
    upstream.write("feature_work.txt", "uncommitted edit on main");
    let repo = Repo::discover(upstream.path()).expect("discover");
    let result = run_checkout(
        &repo,
        &CheckoutPlan::Switch {
            local: "feature".to_string(),
        },
    );
    assert!(result.is_err());
    assert_eq!(head(&repo), Head::Branch("main".to_string()));
}

use kraken::git::{
    RebaseOutcome, ResetMode, abort_rebase, changed_files, commits_leaving, rebase_in_progress,
    rebase_onto, reset_to,
};

fn linear(commits: usize) -> (TestRepo, Vec<String>) {
    let repo = TestRepo::new();
    let hashes = (0..commits)
        .map(|index| repo.commit(&format!("commit {index}")))
        .collect();
    (repo, hashes)
}

#[test]
fn counts_commits_leaving_the_branch_and_changed_files() {
    let (test_repo, hashes) = linear(4);
    test_repo.write("commit_3.txt", "edited");
    let repo = Repo::discover(test_repo.path()).expect("discover");
    assert_eq!(commits_leaving(&repo, &hashes[1]).expect("count"), 2);
    assert_eq!(changed_files(&repo).expect("changed"), 1);
}

#[test]
fn soft_reset_keeps_the_changes_staged() {
    let (test_repo, hashes) = linear(3);
    let repo = Repo::discover(test_repo.path()).expect("discover");
    reset_to(&repo, ResetMode::Soft, &hashes[1]).expect("reset");
    assert_eq!(
        repo.run(&["rev-parse", "HEAD"]).expect("head").trim(),
        hashes[1]
    );
    assert_eq!(
        repo.run(&["diff", "--cached", "--name-only"])
            .expect("staged")
            .trim(),
        "commit_2.txt"
    );
}

#[test]
fn mixed_reset_keeps_the_changes_unstaged() {
    let (test_repo, hashes) = linear(3);
    test_repo.write("commit_1.txt", "edited tracked file");
    let repo = Repo::discover(test_repo.path()).expect("discover");
    reset_to(&repo, ResetMode::Mixed, &hashes[1]).expect("reset");
    assert!(
        repo.run(&["diff", "--cached", "--name-only"])
            .expect("staged")
            .trim()
            .is_empty()
    );
    assert_eq!(
        repo.run(&["diff", "--name-only"]).expect("unstaged").trim(),
        "commit_1.txt"
    );
}

#[test]
fn hard_reset_discards_the_changes() {
    let (test_repo, hashes) = linear(3);
    test_repo.write("commit_1.txt", "edited tracked file");
    let repo = Repo::discover(test_repo.path()).expect("discover");
    reset_to(&repo, ResetMode::Hard, &hashes[1]).expect("reset");
    assert_eq!(changed_files(&repo).expect("changed"), 0);
    assert!(!test_repo.path().join("commit_2.txt").exists());
}

#[test]
fn rebase_replays_the_branch_onto_the_target() {
    let test_repo = TestRepo::new();
    test_repo.commit("base");
    test_repo.branch("feature");
    let main_tip = test_repo.commit("main work");
    test_repo.checkout("feature");
    test_repo.commit("feature work");
    let repo = Repo::discover(test_repo.path()).expect("discover");
    assert_eq!(
        rebase_onto(&repo, "main").expect("rebase"),
        RebaseOutcome::Done
    );
    assert_eq!(
        repo.run(&["rev-parse", "HEAD~1"]).expect("parent").trim(),
        main_tip
    );
}

#[test]
fn conflicting_rebase_stops_and_can_be_aborted() {
    let test_repo = TestRepo::new();
    test_repo.write("shared.txt", "base");
    test_repo.git(&["add", "-A"]);
    test_repo.git(&["commit", "-q", "-m", "base"]);
    test_repo.branch("feature");
    test_repo.write("shared.txt", "main version");
    test_repo.git(&["commit", "-q", "-am", "main change"]);
    test_repo.checkout("feature");
    test_repo.write("shared.txt", "feature version");
    test_repo.git(&["commit", "-q", "-am", "feature change"]);
    let before = test_repo.git(&["rev-parse", "HEAD"]);
    let repo = Repo::discover(test_repo.path()).expect("discover");
    assert_eq!(
        rebase_onto(&repo, "main").expect("rebase"),
        RebaseOutcome::Conflict
    );
    assert!(rebase_in_progress(&repo));
    abort_rebase(&repo).expect("abort");
    assert!(!rebase_in_progress(&repo));
    assert_eq!(test_repo.git(&["rev-parse", "HEAD"]), before);
}
