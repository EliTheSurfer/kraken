use anyhow::Result;

use super::Repo;

#[derive(Clone, Debug, PartialEq)]
pub struct ChangedFile {
    pub status: char,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub message: String,
    pub files: Vec<ChangedFile>,
}

pub fn read_detail(repo: &Repo, hash: &str) -> Result<Detail> {
    let message = repo.run(&["show", "-s", "--format=%B", hash])?;
    let listing = repo.run(&[
        "show",
        "--name-status",
        "--format=",
        "--first-parent",
        "--root",
        "-M",
        hash,
    ])?;
    Ok(Detail {
        message: message.trim_end().to_string(),
        files: listing.lines().filter_map(parse_change).collect(),
    })
}

fn parse_change(line: &str) -> Option<ChangedFile> {
    let mut fields = line.split('\t');
    let status = fields.next()?.chars().next()?;
    let path = fields.next_back()?.to_string();
    Some(ChangedFile { status, path })
}
