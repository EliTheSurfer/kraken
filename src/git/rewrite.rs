use anyhow::Result;

use super::Repo;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RebaseOutcome {
    Done,
    Conflict,
}

pub fn commits_leaving(repo: &Repo, target: &str) -> Result<usize> {
    let count = repo.run(&["rev-list", "--count", &format!("{target}..HEAD")])?;
    Ok(count.trim().parse()?)
}

pub fn changed_files(repo: &Repo) -> Result<usize> {
    let status = repo.run(&["status", "--porcelain", "--untracked-files=no"])?;
    Ok(status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count())
}

pub fn reset_to(repo: &Repo, mode: ResetMode, target: &str) -> Result<String> {
    let flag = match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
    };
    repo.run(&["reset", "--quiet", flag, target])?;
    let short: String = target.chars().take(7).collect();
    Ok(format!("{} reset to {short}", mode_name(mode)))
}

pub fn rebase_onto(repo: &Repo, onto: &str) -> Result<RebaseOutcome> {
    match repo.run(&["rebase", "--quiet", onto]) {
        Ok(_) => Ok(RebaseOutcome::Done),
        Err(_) if rebase_in_progress(repo) => Ok(RebaseOutcome::Conflict),
        Err(error) => Err(error),
    }
}

pub fn rebase_in_progress(repo: &Repo) -> bool {
    ["rebase-merge", "rebase-apply"].iter().any(|name| {
        repo.run(&["rev-parse", "--git-path", name])
            .map(|path| repo.root.join(path.trim()).exists())
            .unwrap_or(false)
    })
}

pub fn abort_rebase(repo: &Repo) -> Result<()> {
    repo.run(&["rebase", "--abort"])?;
    Ok(())
}

fn mode_name(mode: ResetMode) -> &'static str {
    match mode {
        ResetMode::Soft => "Soft",
        ResetMode::Mixed => "Mixed",
        ResetMode::Hard => "Hard",
    }
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
    }
}

pub fn reset_mode_question(
    mode: ResetMode,
    branch: &str,
    target: &str,
    leaving: usize,
    changed: usize,
) -> String {
    let question = format!("{} reset {branch} to {target}?", mode_name(mode));
    let leaving_text = format!("{} will leave the branch", plural(leaving, "commit"));
    match mode {
        ResetMode::Hard => {
            format!(
                "{question} {leaving_text} and {} will be discarded.",
                plural(changed, "changed file")
            )
        }
        ResetMode::Soft if leaving > 0 => {
            format!("{question} {leaving_text}; its changes stay staged.")
        }
        ResetMode::Soft => format!("{question} Changes stay staged."),
        ResetMode::Mixed => format!("{question} Changes stay in your working tree, unstaged."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_reset_question_warns_about_commits_and_discarded_changes() {
        assert_eq!(
            reset_mode_question(ResetMode::Hard, "staging", "abc1234", 3, 2),
            "Hard reset staging to abc1234? 3 commits will leave the branch and 2 changed files will be discarded."
        );
    }

    #[test]
    fn soft_and_mixed_questions_say_changes_are_kept() {
        assert_eq!(
            reset_mode_question(ResetMode::Soft, "staging", "abc1234", 1, 0),
            "Soft reset staging to abc1234? 1 commit will leave the branch; its changes stay staged."
        );
        assert_eq!(
            reset_mode_question(ResetMode::Mixed, "staging", "abc1234", 0, 4),
            "Mixed reset staging to abc1234? Changes stay in your working tree, unstaged."
        );
    }
}
