use md5::{Digest, Md5};

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Url(String),
    GithubCommit {
        owner: String,
        repo: String,
        hash: String,
    },
    GitlabUser {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Forge {
    Github { owner: String, repo: String },
    Gitlab,
    Other,
}

pub fn sources_for(email: &str, name: &str, forge: &Forge, sample_hash: &str) -> Vec<Source> {
    let normalized = email.trim().to_lowercase();
    let mut sources = Vec::new();
    if let Some(id) = github_noreply_id(&normalized) {
        sources.push(Source::Url(format!(
            "https://avatars.githubusercontent.com/u/{id}?s=64"
        )));
    }
    match forge {
        Forge::Github { owner, repo } => sources.push(Source::GithubCommit {
            owner: owner.clone(),
            repo: repo.clone(),
            hash: sample_hash.to_string(),
        }),
        Forge::Gitlab => sources.push(Source::GitlabUser {
            name: name.to_string(),
        }),
        Forge::Other => {}
    }
    sources.push(Source::Url(format!(
        "https://www.gravatar.com/avatar/{}?s=64&d=404",
        hex(&Md5::digest(normalized.as_bytes()))
    )));
    sources
}

pub fn gitlab_avatar_url(search_response: &str, author: &str) -> Option<String> {
    let users: Vec<serde_json::Value> = serde_json::from_str(search_response).ok()?;
    let author = author.trim();
    users
        .iter()
        .filter(|user| {
            let field = |key: &str| {
                user.get(key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
            };
            !field("username").contains("_placeholder_")
                && (field("username").eq_ignore_ascii_case(author)
                    || same_person(field("name"), author))
        })
        .find_map(|user| {
            let url = user.get("avatar_url")?.as_str()?;
            if !url.contains("/uploads/") {
                return None;
            }
            let separator = if url.contains('?') { '&' } else { '?' };
            Some(format!("{url}{separator}width=64"))
        })
}

fn name_words(name: &str) -> Vec<String> {
    let without_suffix = name.split('(').next().unwrap_or(name);
    without_suffix
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn same_person(gitlab_name: &str, author: &str) -> bool {
    let author_words = name_words(author);
    let gitlab_words = name_words(gitlab_name);
    !author_words.is_empty() && author_words.iter().all(|word| gitlab_words.contains(word))
}

fn github_noreply_id(email: &str) -> Option<&str> {
    let local = email.strip_suffix("@users.noreply.github.com")?;
    let (id, _login) = local.split_once('+')?;
    id.chars()
        .all(|character| character.is_ascii_digit())
        .then_some(id)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github() -> Forge {
        Forge::Github {
            owner: "Eli".to_string(),
            repo: "kraken".to_string(),
        }
    }

    #[test]
    fn github_noreply_email_maps_straight_to_the_avatar_url() {
        let sources = sources_for("123+eli@users.noreply.github.com", "Eli", &github(), "abc");
        assert_eq!(
            sources[0],
            Source::Url("https://avatars.githubusercontent.com/u/123?s=64".to_string())
        );
    }

    #[test]
    fn github_repository_asks_the_commit_api_before_gravatar() {
        let sources = sources_for("ada@example.com", "Ada", &github(), "abc");
        assert_eq!(
            sources[0],
            Source::GithubCommit {
                owner: "Eli".to_string(),
                repo: "kraken".to_string(),
                hash: "abc".to_string()
            }
        );
        assert!(
            matches!(&sources[1], Source::Url(url) if url.starts_with("https://www.gravatar.com/avatar/"))
        );
    }

    #[test]
    fn gitlab_repository_searches_the_author_on_gitlab_before_gravatar() {
        let sources = sources_for("grace@example.com", "HOPPER Grace", &Forge::Gitlab, "abc");
        assert_eq!(
            sources[0],
            Source::GitlabUser {
                name: "HOPPER Grace".to_string()
            }
        );
        assert!(
            matches!(&sources[1], Source::Url(url) if url.starts_with("https://www.gravatar.com/avatar/"))
        );
    }

    #[test]
    fn other_hosts_go_straight_to_gravatar_with_md5_of_trimmed_lowercase_email() {
        let sources = sources_for("  MyEmailAddress@Example.com ", "Me", &Forge::Other, "abc");
        assert_eq!(
            sources,
            vec![Source::Url(
                "https://www.gravatar.com/avatar/0bc83cb571cd1c50ba6f3e8a78ef1346?s=64&d=404"
                    .to_string()
            )]
        );
    }

    const SEARCH: &str = r#"[
        {"username":"GHOPPER_placeholder_9trg2m","name":"Placeholder HOPPER Grace","avatar_url":"https://gitlab.example/uploads/-/system/user/avatar/1/avatar.png"},
        {"username":"Ada.Lovelace","name":"Ada Lovelace","avatar_url":"https://gitlab.example/uploads/-/system/user/avatar/36500/avatar.png"},
        {"username":"GHOPPER","name":"HOPPER Grace","avatar_url":"https://secure.gravatar.com/avatar/6194?s=80&d=identicon"}
    ]"#;

    #[test]
    fn gitlab_avatar_is_the_uploaded_photo_of_the_matching_user_at_64_pixels() {
        assert_eq!(
            gitlab_avatar_url(SEARCH, "ada lovelace"),
            Some(
                "https://gitlab.example/uploads/-/system/user/avatar/36500/avatar.png?width=64"
                    .to_string()
            )
        );
    }

    #[test]
    fn gitlab_users_without_a_photo_and_placeholders_are_skipped() {
        assert_eq!(gitlab_avatar_url(SEARCH, "HOPPER Grace"), None);
        assert_eq!(gitlab_avatar_url(SEARCH, "Placeholder HOPPER Grace"), None);
    }

    #[test]
    fn gitlab_name_matches_regardless_of_word_order_case_and_company_suffix() {
        let search = r#"[
            {"username":"AKAY_placeholder_154dyz4","name":"Placeholder KAY Alan (Contractor for Example Corp)","avatar_url":"https://gitlab.example/uploads/p.png"},
            {"username":"AKAY","name":"KAY Alan (Contractor for Example Corp)","avatar_url":"https://gitlab.example/uploads/r.png"}
        ]"#;
        assert_eq!(
            gitlab_avatar_url(search, "Alan KAY"),
            Some("https://gitlab.example/uploads/r.png?width=64".to_string())
        );
    }

    #[test]
    fn gitlab_name_needs_every_author_word() {
        let search = r#"[{"username":"ADUPONT","name":"DUPONT Alan","avatar_url":"https://gitlab.example/uploads/d.png"}]"#;
        assert_eq!(gitlab_avatar_url(search, "Alan KAY"), None);
    }

    #[test]
    fn gitlab_author_named_by_username_or_email_still_matches() {
        let search = r#"[{"username":"Ada.Lovelace","name":"Ada.Lovelace@example.com","avatar_url":"https://gitlab.example/uploads/a.png"}]"#;
        assert_eq!(
            gitlab_avatar_url(search, "Ada.Lovelace@example.com"),
            Some("https://gitlab.example/uploads/a.png?width=64".to_string())
        );
    }
}
