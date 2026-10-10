use crate::graph::PALETTE_SIZE;

pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let first_letter = |word: &str| {
        word.chars()
            .find(|character| character.is_alphanumeric())
            .map(|character| character.to_uppercase().to_string())
    };
    let Some(first) = words.first().and_then(|word| first_letter(word)) else {
        return "?".to_string();
    };
    let last = words
        .iter()
        .skip(1)
        .last()
        .and_then(|word| first_letter(word))
        .unwrap_or_default();
    first + &last
}

pub fn author_color(email: &str) -> usize {
    let hash = email
        .trim()
        .to_lowercase()
        .bytes()
        .fold(2_166_136_261_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
        });
    hash as usize % PALETTE_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_take_first_and_last_words() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("ada lovelace byron"), "AB");
    }

    #[test]
    fn initials_of_a_single_word_or_bot_name() {
        assert_eq!(initials("renovate[bot]"), "R");
        assert_eq!(initials(""), "?");
    }

    #[test]
    fn author_color_is_stable_and_spreads_authors() {
        assert_eq!(
            author_color("ada@example.com"),
            author_color("ada@example.com")
        );
        let colors: std::collections::HashSet<usize> =
            ["a@x.io", "b@x.io", "c@x.io", "d@x.io", "e@x.io", "f@x.io"]
                .iter()
                .map(|email| author_color(email))
                .collect();
        assert!(colors.len() > 1);
    }
}
