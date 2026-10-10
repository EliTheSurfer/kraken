#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntryKind {
    Commit,
    Local,
    Remote,
    Tag,
    Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
    pub detail: String,
    pub target: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub entry: usize,
    pub label_matches: Vec<usize>,
}

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub fn rank(query: &str, entries: &[Entry]) -> Vec<Ranked> {
    if query.trim().is_empty() {
        return (0..entries.len())
            .map(|entry| Ranked {
                entry,
                label_matches: Vec::new(),
            })
            .collect();
    }
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buffer = Vec::new();
    let mut indices = Vec::new();
    let mut scored: Vec<(u32, Ranked)> = entries
        .iter()
        .enumerate()
        .filter_map(|(position, entry)| {
            let haystack = format!("{} {}", entry.label, entry.detail);
            indices.clear();
            let score = pattern.indices(
                Utf32Str::new(&haystack, &mut buffer),
                &mut matcher,
                &mut indices,
            )?;
            let label_length = entry.label.chars().count() as u32;
            let mut label_matches: Vec<usize> = indices
                .iter()
                .filter(|&&index| index < label_length)
                .map(|&index| index as usize)
                .collect();
            label_matches.sort_unstable();
            label_matches.dedup();
            Some((
                score,
                Ranked {
                    entry: position,
                    label_matches,
                },
            ))
        })
        .collect();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.entry.cmp(&right.1.entry)));
    scored.into_iter().map(|(_, ranked)| ranked).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: EntryKind, label: &str, detail: &str) -> Entry {
        Entry {
            kind,
            label: label.to_string(),
            detail: detail.to_string(),
            target: label.to_string(),
        }
    }

    fn labels(query: &str, entries: &[Entry]) -> Vec<String> {
        rank(query, entries)
            .into_iter()
            .map(|ranked| entries[ranked.entry].label.clone())
            .collect()
    }

    fn sample() -> Vec<Entry> {
        vec![
            entry(EntryKind::Local, "staging", ""),
            entry(EntryKind::Tag, "v1.18.14", ""),
            entry(
                EntryKind::Commit,
                "FEAT-827 - make the image tags optional",
                "97be206 · Ada Lovelace",
            ),
            entry(
                EntryKind::Commit,
                "Update all minor dependencies",
                "1a2b3c4 · Renovate Bot",
            ),
        ]
    }

    #[test]
    fn empty_query_keeps_every_entry_in_order() {
        assert_eq!(labels("", &sample()).len(), 4);
        assert_eq!(labels("", &sample())[0], "staging");
    }

    #[test]
    fn letters_in_order_match_with_gaps() {
        assert_eq!(
            labels("feat827opt", &sample()),
            vec!["FEAT-827 - make the image tags optional".to_string()]
        );
    }

    #[test]
    fn commits_match_on_hash_and_author_too() {
        assert_eq!(
            labels("97be206", &sample()),
            vec!["FEAT-827 - make the image tags optional".to_string()]
        );
        assert_eq!(
            labels("renovate", &sample()),
            vec!["Update all minor dependencies".to_string()]
        );
    }

    #[test]
    fn closer_matches_rank_first() {
        let entries = vec![
            entry(EntryKind::Local, "feat/staging-cleanup", ""),
            entry(EntryKind::Local, "staging", ""),
        ];
        assert_eq!(labels("staging", &entries)[0], "staging");
    }

    #[test]
    fn matched_letters_are_reported_within_the_label() {
        let ranked = rank("stg", &sample());
        assert_eq!(ranked[0].label_matches, vec![0, 1, 3]);
    }
}
