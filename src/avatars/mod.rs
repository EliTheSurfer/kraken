pub mod cache;
pub mod initials;
pub mod resolve;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use image::RgbaImage;
use image::imageops::FilterType;

use cache::{Cache, Cached};
pub use initials::{author_color, initials};
use resolve::{Forge, Source, gitlab_avatar_url, sources_for};

pub const AVATAR_SIZE: u32 = 64;

pub struct Request {
    pub email: String,
    pub name: String,
    pub sample_hash: String,
}

pub struct AvatarStore {
    requests: Sender<Request>,
    ready: Receiver<(String, RgbaImage)>,
    requested: HashSet<String>,
}

impl AvatarStore {
    pub fn start(repo_root: PathBuf, forge: Forge, cache: Cache) -> AvatarStore {
        let (requests, request_receiver) = channel::<Request>();
        let (ready_sender, ready) = channel();
        thread::spawn(move || {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .build()
                .into();
            for request in request_receiver {
                let email = request.email.trim().to_lowercase();
                let avatar = match cache.load(&email) {
                    Cached::Found(avatar) => Some(avatar),
                    Cached::Missing => None,
                    Cached::Unknown => {
                        let fetched =
                            sources_for(&email, &request.name, &forge, &request.sample_hash)
                                .into_iter()
                                .find_map(|source| fetch(&agent, &repo_root, source));
                        match &fetched {
                            Some(avatar) => cache.store_found(&email, avatar),
                            None => cache.store_missing(&email),
                        }
                        fetched
                    }
                };
                if let Some(avatar) = avatar
                    && ready_sender.send((email, avatar)).is_err()
                {
                    return;
                }
            }
        });
        AvatarStore {
            requests,
            ready,
            requested: HashSet::new(),
        }
    }

    pub fn request(&mut self, email: &str, name: &str, sample_hash: &str) {
        let email = email.trim().to_lowercase();
        if self.requested.insert(email.clone()) {
            let _ = self.requests.send(Request {
                email,
                name: name.to_string(),
                sample_hash: sample_hash.to_string(),
            });
        }
    }

    pub fn poll(&self) -> Vec<(String, RgbaImage)> {
        self.ready.try_iter().collect()
    }
}

fn fetch(agent: &ureq::Agent, repo_root: &PathBuf, source: Source) -> Option<RgbaImage> {
    let url = match source {
        Source::Url(url) => url,
        Source::GithubCommit { owner, repo, hash } => {
            github_avatar_url(repo_root, &owner, &repo, &hash)?
        }
        Source::GitlabUser { name } => gitlab_user_avatar_url(repo_root, &name)?,
    };
    let bytes = agent.get(&url).call().ok()?.body_mut().read_to_vec().ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?;
    Some(
        decoded
            .resize_to_fill(AVATAR_SIZE, AVATAR_SIZE, FilterType::Lanczos3)
            .to_rgba8(),
    )
}

fn gitlab_user_avatar_url(repo_root: &PathBuf, name: &str) -> Option<String> {
    let query: String = url_encode(name);
    let output = std::process::Command::new("glab")
        .current_dir(repo_root)
        .args(["api", &format!("users?search={query}&per_page=20")])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    gitlab_avatar_url(&String::from_utf8_lossy(&output.stdout), name)
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn github_avatar_url(repo_root: &PathBuf, owner: &str, repo: &str, hash: &str) -> Option<String> {
    let output = std::process::Command::new("gh")
        .current_dir(repo_root)
        .args([
            "api",
            &format!("repos/{owner}/{repo}/commits/{hash}"),
            "--jq",
            ".author.avatar_url",
        ])
        .output()
        .ok()?;
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && url.starts_with("https://")).then_some(url)
}
