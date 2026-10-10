#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

pub struct TestRepo {
    dir: TempDir,
}

impl TestRepo {
    pub fn new() -> TestRepo {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = TestRepo { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.name", "Ada Lovelace"]);
        repo.git(&["config", "user.email", "ada@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "tag.gpgsign", "false"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.dir.path())
            .args(args)
            .env("GIT_COMMITTER_DATE", "2026-10-10T10:00:00Z")
            .env("GIT_AUTHOR_DATE", "2026-10-10T10:00:00Z")
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    pub fn write(&self, file: &str, content: &str) {
        let path = self.dir.path().join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, content).expect("write");
    }

    pub fn commit(&self, message: &str) -> String {
        self.write(&format!("{}.txt", message.replace(' ', "_")), message);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn branch(&self, name: &str) {
        self.git(&["branch", name]);
    }

    pub fn checkout(&self, name: &str) {
        self.git(&["checkout", "-q", name]);
    }

    pub fn merge(&self, branch: &str, message: &str) -> String {
        self.git(&["merge", "-q", "--no-ff", "-m", message, branch]);
        self.git(&["rev-parse", "HEAD"])
    }

    pub fn tag_annotated(&self, name: &str) {
        self.git(&["tag", "-a", name, "-m", name]);
    }
}
