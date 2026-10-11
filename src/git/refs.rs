use anyhow::Result;

use super::Repo;

const REF_FORMAT: &str = "--format=%(refname)%1f%(objectname)%1f%(*objectname)";

#[derive(Clone, Debug, PartialEq)]
pub enum RefKind {
    Local,
    Remote { remote: String },
    Tag,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ref {
    pub name: String,
    pub kind: RefKind,
    pub target: String,
}

impl Ref {
    pub fn short_name(&self) -> &str {
        match &self.kind {
            RefKind::Remote { remote } => self
                .name
                .strip_prefix(remote.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
                .unwrap_or(&self.name),
            RefKind::Local | RefKind::Tag => &self.name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RefBadge {
    pub name: String,
    pub local: bool,
    pub remote: bool,
    pub tag: bool,
}

pub fn ref_badges(refs: &[&Ref]) -> Vec<RefBadge> {
    refs.iter().fold(Vec::new(), |mut badges, reference| {
        let tag = reference.kind == RefKind::Tag;
        let local = reference.kind == RefKind::Local;
        let name = reference.short_name();
        let existing = badges
            .iter_mut()
            .find(|badge: &&mut RefBadge| !tag && !badge.tag && badge.name == name);
        match existing {
            Some(badge) => {
                badge.local |= local;
                badge.remote |= !local;
            }
            None => badges.push(RefBadge {
                name: name.to_string(),
                local,
                remote: !local && !tag,
                tag,
            }),
        }
        badges
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum Head {
    Branch(String),
    Detached(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Refs {
    pub refs: Vec<Ref>,
    pub head: Head,
}

pub fn read_refs(repo: &Repo) -> Result<Refs> {
    let listing = repo.run(&[
        "for-each-ref",
        REF_FORMAT,
        "refs/heads",
        "refs/remotes",
        "refs/tags",
    ])?;
    let refs = listing.lines().filter_map(parse_ref).collect();
    Ok(Refs {
        refs,
        head: read_head(repo),
    })
}

fn parse_ref(line: &str) -> Option<Ref> {
    let mut fields = line.split('\u{1f}');
    let full_name = fields.next()?;
    let object = fields.next()?;
    let peeled = fields.next().unwrap_or("");
    let target = if peeled.is_empty() { object } else { peeled }.to_string();
    if let Some(name) = full_name.strip_prefix("refs/heads/") {
        return Some(Ref {
            name: name.to_string(),
            kind: RefKind::Local,
            target,
        });
    }
    if let Some(name) = full_name.strip_prefix("refs/tags/") {
        return Some(Ref {
            name: name.to_string(),
            kind: RefKind::Tag,
            target,
        });
    }
    let name = full_name.strip_prefix("refs/remotes/")?;
    let (remote, branch) = name.split_once('/')?;
    if branch == "HEAD" {
        return None;
    }
    Some(Ref {
        name: name.to_string(),
        kind: RefKind::Remote {
            remote: remote.to_string(),
        },
        target,
    })
}

fn read_head(repo: &Repo) -> Head {
    if let Ok(branch) = repo.run(&["symbolic-ref", "-q", "--short", "HEAD"]) {
        return Head::Branch(branch.trim().to_string());
    }
    let hash = repo.run(&["rev-parse", "HEAD"]).unwrap_or_default();
    Head::Detached(hash.trim().to_string())
}
