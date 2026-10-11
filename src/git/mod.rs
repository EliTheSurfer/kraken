use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub mod checkout;
pub mod log;
pub mod refs;
pub mod rewrite;
pub mod show;

pub use checkout::{
    CheckoutPlan, commits_dropped_by_reset, plan_checkout, reset_question, run_checkout,
};
pub use log::{Commit, CommitStream, Person};
pub use refs::{Head, Ref, RefBadge, RefKind, Refs, read_refs, ref_badges};
pub use rewrite::{
    RebaseOutcome, ResetMode, abort_rebase, changed_files, commits_leaving, rebase_in_progress,
    rebase_onto, reset_mode_question, reset_to,
};
pub use show::{ChangedFile, Detail, read_detail};

pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    pub fn discover(start: &Path) -> Result<Repo> {
        let root = git_output(start, &["rev-parse", "--show-toplevel"])
            .with_context(|| format!("{} is not inside a git repository", start.display()))?;
        Ok(Repo {
            root: PathBuf::from(root.trim()),
        })
    }

    pub fn run(&self, args: &[&str]) -> Result<String> {
        git_output(&self.root, args)
    }
}

pub fn command(dir: &Path, args: &[&str]) -> Command {
    let mut git = Command::new("git");
    git.arg("-C").arg(dir).args(args).env("LC_ALL", "C");
    git
}

fn git_output(dir: &Path, args: &[&str]) -> Result<String> {
    let output = command(dir, args)
        .output()
        .context("cannot run git, is it installed?")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(dir.path())
            .status()
            .expect("git init");
        assert!(status.success());
        dir
    }

    #[test]
    fn discover_finds_repository_root_from_subdirectory() {
        let dir = init_repo();
        let nested = dir.path().join("a/b");
        std::fs::create_dir_all(&nested).expect("mkdir");
        let repo = Repo::discover(&nested).expect("discover");
        assert_eq!(
            repo.root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn discover_fails_outside_a_repository() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = dir.path().join("plain");
        std::fs::create_dir_all(&outside).expect("mkdir");
        let ceiling = dir.path().to_string_lossy().to_string();
        unsafe { std::env::set_var("GIT_CEILING_DIRECTORIES", ceiling) };
        assert!(Repo::discover(&outside).is_err());
    }

    #[test]
    fn run_returns_stdout_and_reports_failures() {
        let dir = init_repo();
        let repo = Repo {
            root: dir.path().to_path_buf(),
        };
        assert!(
            repo.run(&["rev-parse", "--is-inside-work-tree"])
                .expect("run")
                .trim()
                == "true"
        );
        assert!(repo.run(&["rev-parse", "--verify", "missing-ref"]).is_err());
    }
}
