use std::path::PathBuf;

use image::RgbaImage;
use sha2::{Digest, Sha256};

use super::resolve::hex;

const MISSING_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

#[derive(Debug, PartialEq)]
pub enum Cached {
    Found(RgbaImage),
    Missing,
    Unknown,
}

pub struct Cache {
    pub dir: PathBuf,
}

impl Cache {
    pub fn default_dir() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join(".cache").join("kraken").join("avatars")
    }

    pub fn load(&self, email: &str) -> Cached {
        let key = key(email);
        if let Ok(avatar) = image::open(self.dir.join(format!("{key}.png"))) {
            return Cached::Found(avatar.to_rgba8());
        }
        let marker = self.dir.join(format!("{key}.none"));
        let fresh = std::fs::metadata(marker)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age < MISSING_TTL);
        if fresh {
            return Cached::Missing;
        }
        Cached::Unknown
    }

    pub fn store_found(&self, email: &str, avatar: &RgbaImage) {
        if std::fs::create_dir_all(&self.dir).is_ok() {
            let _ = avatar.save(self.dir.join(format!("{}.png", key(email))));
        }
    }

    pub fn store_missing(&self, email: &str) {
        if std::fs::create_dir_all(&self.dir).is_ok() {
            let _ = std::fs::write(self.dir.join(format!("{}.none", key(email))), b"");
        }
    }
}

fn key(email: &str) -> String {
    hex(&Sha256::digest(email.trim().to_lowercase().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> (tempfile::TempDir, Cache) {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = Cache {
            dir: dir.path().join("avatars"),
        };
        (dir, cache)
    }

    #[test]
    fn unknown_email_is_neither_found_nor_missing() {
        let (_dir, cache) = cache();
        assert_eq!(cache.load("ada@example.com"), Cached::Unknown);
    }

    #[test]
    fn found_avatar_round_trips_through_disk() {
        let (_dir, cache) = cache();
        let avatar = RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
        cache.store_found("ada@example.com", &avatar);
        assert_eq!(cache.load("ada@example.com"), Cached::Found(avatar));
    }

    #[test]
    fn missing_avatar_is_retried_after_a_day() {
        let (_dir, cache) = cache();
        cache.store_missing("ada@example.com");
        let marker = std::fs::read_dir(&cache.dir)
            .expect("dir")
            .next()
            .expect("entry")
            .expect("entry")
            .path();
        let two_days_ago =
            std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 24 * 3600);
        std::fs::File::options()
            .write(true)
            .open(marker)
            .expect("open")
            .set_modified(two_days_ago)
            .expect("mtime");
        assert_eq!(cache.load("ada@example.com"), Cached::Unknown);
    }

    #[test]
    fn missing_avatar_is_remembered_case_insensitively() {
        let (_dir, cache) = cache();
        cache.store_missing("Ada@Example.com");
        assert_eq!(cache.load("ada@example.com"), Cached::Missing);
    }
}
