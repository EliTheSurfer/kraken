use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use crate::git::{Detail, Repo, read_detail};

pub enum Outcome<T> {
    Loaded(T),
    Failed(String),
    Skipped,
}

pub fn latest_request(first: String, pending: &Receiver<String>) -> (String, Vec<String>) {
    let mut skipped = Vec::new();
    let mut latest = first;
    for newer in pending.try_iter() {
        skipped.push(std::mem::replace(&mut latest, newer));
    }
    (latest, skipped)
}

pub struct BackgroundLoader<T> {
    requests: Sender<String>,
    results: Receiver<(String, Outcome<T>)>,
    in_flight: HashSet<String>,
    failed: HashSet<String>,
    loaded: HashMap<String, T>,
}

pub type DetailLoader = BackgroundLoader<Detail>;

impl BackgroundLoader<Detail> {
    pub fn start(repo_root: PathBuf) -> DetailLoader {
        let repo = Repo { root: repo_root };
        BackgroundLoader::with_fetch(move |hash| read_detail(&repo, hash))
    }
}

impl<T: Send + 'static> BackgroundLoader<T> {
    pub fn with_fetch(
        fetch: impl Fn(&str) -> anyhow::Result<T> + Send + 'static,
    ) -> BackgroundLoader<T> {
        let (requests, request_receiver) = channel::<String>();
        let (result_sender, results) = channel();
        thread::spawn(move || {
            while let Ok(first) = request_receiver.recv() {
                let (key, skipped) = latest_request(first, &request_receiver);
                for skipped_key in skipped {
                    if result_sender.send((skipped_key, Outcome::Skipped)).is_err() {
                        return;
                    }
                }
                let outcome = match fetch(&key) {
                    Ok(value) => Outcome::Loaded(value),
                    Err(error) => Outcome::Failed(format!("{error:#}")),
                };
                if result_sender.send((key, outcome)).is_err() {
                    return;
                }
            }
        });
        BackgroundLoader {
            requests,
            results,
            in_flight: HashSet::new(),
            failed: HashSet::new(),
            loaded: HashMap::new(),
        }
    }

    pub fn want(&mut self, key: &str) {
        if self.loaded.contains_key(key)
            || self.in_flight.contains(key)
            || self.failed.contains(key)
        {
            return;
        }
        if self.requests.send(key.to_string()).is_ok() {
            self.in_flight.insert(key.to_string());
        }
    }

    pub fn poll(&mut self) -> Vec<(String, String)> {
        let mut errors = Vec::new();
        for (key, outcome) in self.results.try_iter() {
            self.in_flight.remove(&key);
            match outcome {
                Outcome::Loaded(value) => {
                    self.loaded.insert(key, value);
                }
                Outcome::Failed(error) => {
                    self.failed.insert(key.clone());
                    errors.push((key, error));
                }
                Outcome::Skipped => {}
            }
        }
        errors
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.loaded.get(key)
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::*;

    #[test]
    fn latest_request_keeps_the_newest_and_reports_the_skipped_ones() {
        let (sender, receiver) = channel();
        for hash in ["b", "c", "d"] {
            sender.send(hash.to_string()).expect("send");
        }
        assert_eq!(
            latest_request("a".to_string(), &receiver),
            ("d".to_string(), vec!["a".into(), "b".into(), "c".into()])
        );
    }

    #[test]
    fn the_same_hash_is_requested_once_while_in_flight() {
        let mut loader = DetailLoader::start(PathBuf::from("/nonexistent-kraken-repo"));
        loader.want("abc");
        loader.want("abc");
        assert_eq!(loader.in_flight(), 1);
    }

    #[test]
    fn a_failed_load_is_reported_and_leaves_the_in_flight_set() {
        let mut loader = DetailLoader::start(PathBuf::from("/nonexistent-kraken-repo"));
        loader.want("abc");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut errors = Vec::new();
        while errors.is_empty() && std::time::Instant::now() < deadline {
            errors = loader.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(errors.len(), 1);
        assert_eq!(loader.in_flight(), 0);
        loader.want("abc");
        assert_eq!(loader.in_flight(), 0);
    }
}
