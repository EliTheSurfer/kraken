mod support;

use kraken::git::{CommitStream, Head, RefKind, Repo, read_refs};
use support::TestRepo;

fn open(test_repo: &TestRepo) -> Repo {
    Repo::discover(test_repo.path()).expect("discover")
}

#[test]
fn commits_come_newest_first_with_parents() {
    let test_repo = TestRepo::new();
    let first = test_repo.commit("first");
    let second = test_repo.commit("second");
    let third = test_repo.commit("third");
    let mut stream = CommitStream::spawn(&open(&test_repo)).expect("spawn");
    let commits = stream.next_batch(10).expect("batch");
    let hashes: Vec<&str> = commits.iter().map(|commit| commit.hash.as_str()).collect();
    assert_eq!(
        hashes,
        vec![third.as_str(), second.as_str(), first.as_str()]
    );
    assert_eq!(commits[0].parents, vec![second.clone()]);
    assert!(commits[2].parents.is_empty());
}

#[test]
fn batches_split_the_history() {
    let test_repo = TestRepo::new();
    for index in 0..3 {
        test_repo.commit(&format!("commit {index}"));
    }
    let mut stream = CommitStream::spawn(&open(&test_repo)).expect("spawn");
    assert_eq!(stream.next_batch(2).expect("first batch").len(), 2);
    assert_eq!(stream.next_batch(2).expect("second batch").len(), 1);
    assert!(stream.next_batch(2).expect("drained").is_empty());
}

#[test]
fn merge_commit_has_both_parents_and_metadata() {
    let test_repo = TestRepo::new();
    let base = test_repo.commit("base");
    test_repo.branch("feature");
    test_repo.checkout("feature");
    let feature = test_repo.commit("feature work");
    test_repo.checkout("main");
    let main_work = test_repo.commit("main work");
    let merge = test_repo.merge("feature", "Merge feature | ünïcode");
    let mut stream = CommitStream::spawn(&open(&test_repo)).expect("spawn");
    let commits = stream.next_batch(10).expect("batch");
    let merge_commit = commits
        .iter()
        .find(|commit| commit.hash == merge)
        .expect("merge commit");
    assert_eq!(merge_commit.parents, vec![main_work, feature]);
    assert_eq!(merge_commit.subject, "Merge feature | ünïcode");
    assert_eq!(merge_commit.author.name, "Ada Lovelace");
    assert_eq!(merge_commit.author.email, "ada@example.com");
    assert!(merge_commit.author.time > 0);
    assert_eq!(commits.last().expect("root").hash, base);
}

#[test]
fn empty_repository_has_no_commits() {
    let test_repo = TestRepo::new();
    let mut stream = CommitStream::spawn(&open(&test_repo)).expect("spawn");
    assert!(stream.next_batch(10).expect("batch").is_empty());
}

#[test]
fn refs_include_branches_and_peeled_annotated_tags() {
    let test_repo = TestRepo::new();
    let tagged = test_repo.commit("tagged");
    test_repo.tag_annotated("v1.0");
    test_repo.branch("feature");
    let refs = read_refs(&open(&test_repo)).expect("refs");
    let tag = refs
        .refs
        .iter()
        .find(|reference| reference.name == "v1.0")
        .expect("tag");
    assert_eq!(tag.kind, RefKind::Tag);
    assert_eq!(tag.target, tagged);
    assert!(
        refs.refs
            .iter()
            .any(|reference| reference.name == "feature" && reference.kind == RefKind::Local)
    );
    assert_eq!(refs.head, Head::Branch("main".to_string()));
}

#[test]
fn detached_head_is_reported_with_its_hash() {
    let test_repo = TestRepo::new();
    let first = test_repo.commit("first");
    test_repo.commit("second");
    test_repo.checkout(&first);
    let refs = read_refs(&open(&test_repo)).expect("refs");
    assert_eq!(refs.head, Head::Detached(first));
}

#[test]
fn remote_branches_carry_their_remote_and_skip_remote_head() {
    let upstream = TestRepo::new();
    upstream.commit("upstream");
    let clone_dir = tempfile::tempdir().expect("tempdir");
    let clone_path = clone_dir.path().join("clone");
    let status = std::process::Command::new("git")
        .args(["clone", "-q"])
        .arg(upstream.path())
        .arg(&clone_path)
        .status()
        .expect("clone");
    assert!(status.success());
    let refs = read_refs(&Repo::discover(&clone_path).expect("discover")).expect("refs");
    let remote_refs: Vec<_> = refs
        .refs
        .iter()
        .filter(|reference| matches!(reference.kind, RefKind::Remote { .. }))
        .collect();
    assert_eq!(remote_refs.len(), 1);
    assert_eq!(remote_refs[0].name, "origin/main");
    assert_eq!(
        remote_refs[0].kind,
        RefKind::Remote {
            remote: "origin".to_string()
        }
    );
}

fn statuses(detail: &kraken::git::Detail) -> Vec<(char, String)> {
    detail
        .files
        .iter()
        .map(|file| (file.status, file.path.clone()))
        .collect()
}

#[test]
fn detail_lists_added_modified_and_deleted_files() {
    let test_repo = TestRepo::new();
    test_repo.write("keep.txt", "one");
    test_repo.write("gone.txt", "bye");
    test_repo.git(&["add", "-A"]);
    test_repo.git(&["commit", "-q", "-m", "base"]);
    test_repo.write("keep.txt", "two");
    test_repo.write("src/new.rs", "fn main() {}");
    test_repo.git(&["rm", "-q", "gone.txt"]);
    test_repo.git(&["add", "-A"]);
    test_repo.git(&["commit", "-q", "-m", "change"]);
    let hash = test_repo.git(&["rev-parse", "HEAD"]);
    let detail = kraken::git::read_detail(&open(&test_repo), &hash).expect("detail");
    assert_eq!(
        statuses(&detail),
        vec![
            ('D', "gone.txt".to_string()),
            ('M', "keep.txt".to_string()),
            ('A', "src/new.rs".to_string())
        ]
    );
}

#[test]
fn detail_reports_renames_with_the_new_path() {
    let test_repo = TestRepo::new();
    test_repo.write(
        "old_name.txt",
        "same content that is long enough to be detected as a rename",
    );
    test_repo.git(&["add", "-A"]);
    test_repo.git(&["commit", "-q", "-m", "base"]);
    test_repo.git(&["mv", "old_name.txt", "new_name.txt"]);
    test_repo.git(&["commit", "-q", "-m", "rename"]);
    let hash = test_repo.git(&["rev-parse", "HEAD"]);
    let detail = kraken::git::read_detail(&open(&test_repo), &hash).expect("detail");
    assert_eq!(statuses(&detail), vec![('R', "new_name.txt".to_string())]);
}

#[test]
fn detail_keeps_the_full_message_and_works_on_the_root_commit() {
    let test_repo = TestRepo::new();
    test_repo.write("a.txt", "a");
    test_repo.git(&["add", "-A"]);
    test_repo.git(&[
        "commit",
        "-q",
        "-m",
        "Subject line",
        "-m",
        "Body paragraph\nsecond line",
    ]);
    let hash = test_repo.git(&["rev-parse", "HEAD"]);
    let detail = kraken::git::read_detail(&open(&test_repo), &hash).expect("detail");
    assert_eq!(
        detail.message,
        "Subject line\n\nBody paragraph\nsecond line"
    );
    assert_eq!(statuses(&detail), vec![('A', "a.txt".to_string())]);
}
