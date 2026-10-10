use anyhow::Result;

use super::{Head, Ref, RefKind, Refs, Repo};

#[derive(Clone, Debug, PartialEq)]
pub enum CheckoutPlan {
    Switch {
        local: String,
    },
    Track {
        local: String,
        remote: String,
    },
    Reset {
        local: String,
        remote: String,
        switch_first: bool,
    },
    NotABranch,
}

pub fn plan_checkout(refs: &Refs, chosen: &Ref) -> CheckoutPlan {
    let remote_name = match &chosen.kind {
        RefKind::Tag => return CheckoutPlan::NotABranch,
        RefKind::Local => {
            return CheckoutPlan::Switch {
                local: chosen.name.clone(),
            };
        }
        RefKind::Remote { remote } => remote,
    };
    let local = chosen
        .name
        .strip_prefix(&format!("{remote_name}/"))
        .unwrap_or(&chosen.name)
        .to_string();
    let local_exists = refs
        .refs
        .iter()
        .any(|reference| reference.kind == RefKind::Local && reference.name == local);
    if !local_exists {
        return CheckoutPlan::Track {
            local,
            remote: chosen.name.clone(),
        };
    }
    let switch_first = refs.head != Head::Branch(local.clone());
    CheckoutPlan::Reset {
        local,
        remote: chosen.name.clone(),
        switch_first,
    }
}

pub fn reset_question(local: &str, remote: &str, dropped: usize) -> String {
    let question = format!("Reset {local} to {remote}?");
    match dropped {
        0 => question,
        1 => format!("{question} 1 local commit not on the remote will be dropped."),
        count => format!("{question} {count} local commits not on the remote will be dropped."),
    }
}

pub fn commits_dropped_by_reset(repo: &Repo, local: &str, remote: &str) -> Result<usize> {
    let count = repo.run(&["rev-list", "--count", &format!("{remote}..{local}")])?;
    Ok(count.trim().parse()?)
}

pub fn run_checkout(repo: &Repo, plan: &CheckoutPlan) -> Result<String> {
    match plan {
        CheckoutPlan::Switch { local } => {
            repo.run(&["switch", "--quiet", local])?;
            Ok(format!("switched to {local}"))
        }
        CheckoutPlan::Track { local, remote } => {
            repo.run(&["switch", "--quiet", "--create", local, "--track", remote])?;
            Ok(format!("created {local} tracking {remote}"))
        }
        CheckoutPlan::Reset {
            local,
            remote,
            switch_first,
        } => {
            if *switch_first {
                repo.run(&["switch", "--quiet", local])?;
            }
            repo.run(&["reset", "--quiet", "--keep", remote])?;
            Ok(format!("reset {local} to {remote}"))
        }
        CheckoutPlan::NotABranch => anyhow::bail!("not a branch"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, kind: RefKind) -> Ref {
        Ref {
            name: name.to_string(),
            kind,
            target: "abc".to_string(),
        }
    }

    fn remote(name: &str) -> Ref {
        branch(
            name,
            RefKind::Remote {
                remote: "origin".to_string(),
            },
        )
    }

    fn refs(refs: Vec<Ref>, head: &str) -> Refs {
        Refs {
            refs,
            head: Head::Branch(head.to_string()),
        }
    }

    #[test]
    fn local_branch_is_switched_to() {
        let all = refs(vec![branch("feat/x", RefKind::Local)], "main");
        assert_eq!(
            plan_checkout(&all, &all.refs[0]),
            CheckoutPlan::Switch {
                local: "feat/x".to_string()
            }
        );
    }

    #[test]
    fn remote_branch_without_local_creates_a_tracking_branch() {
        let all = refs(vec![remote("origin/feat/x")], "main");
        assert_eq!(
            plan_checkout(&all, &all.refs[0]),
            CheckoutPlan::Track {
                local: "feat/x".to_string(),
                remote: "origin/feat/x".to_string()
            }
        );
    }

    #[test]
    fn remote_branch_with_a_local_counterpart_resets_it_and_switches_when_needed() {
        let all = refs(
            vec![branch("staging", RefKind::Local), remote("origin/staging")],
            "main",
        );
        assert_eq!(
            plan_checkout(&all, &all.refs[1]),
            CheckoutPlan::Reset {
                local: "staging".to_string(),
                remote: "origin/staging".to_string(),
                switch_first: true
            }
        );
        let on_staging = refs(all.refs.clone(), "staging");
        assert_eq!(
            plan_checkout(&on_staging, &on_staging.refs[1]),
            CheckoutPlan::Reset {
                local: "staging".to_string(),
                remote: "origin/staging".to_string(),
                switch_first: false
            }
        );
    }

    #[test]
    fn reset_question_mentions_the_commits_that_would_be_dropped() {
        assert_eq!(
            reset_question("staging", "origin/staging", 0),
            "Reset staging to origin/staging?"
        );
        assert_eq!(
            reset_question("staging", "origin/staging", 2),
            "Reset staging to origin/staging? 2 local commits not on the remote will be dropped."
        );
        assert_eq!(
            reset_question("staging", "origin/staging", 1),
            "Reset staging to origin/staging? 1 local commit not on the remote will be dropped."
        );
    }

    #[test]
    fn tags_are_not_checked_out() {
        let all = refs(vec![branch("v1", RefKind::Tag)], "main");
        assert_eq!(plan_checkout(&all, &all.refs[0]), CheckoutPlan::NotABranch);
    }
}
