//! Select a bounded excerpt from the whole extracted body using query terms.
//! Short documents and unmatched queries keep the original text order.

use std::collections::HashSet;

pub(super) fn start(text: &str, query: &str, max_chars: usize) -> usize {
    if text.len() <= max_chars || max_chars < 400 {
        return 0;
    }
    let terms: HashSet<_> = words(query)
        .filter(|word| word.len() >= 4)
        .filter(|word| {
            !matches!(
                word.as_str(),
                "about"
                    | "does"
                    | "from"
                    | "that"
                    | "this"
                    | "under"
                    | "what"
                    | "when"
                    | "where"
                    | "which"
                    | "with"
            )
        })
        .take(32)
        .collect();
    if terms.len() < 2 {
        return 0;
    }
    let window = (max_chars / 3).clamp(300, 1200);
    let mut best = (0, 0);
    for offset in (0..text.len()).step_by(window / 2) {
        let begin = crate::text::prefix(text, offset).len();
        let snippet = crate::text::prefix(&text[begin..], window);
        let matches: Vec<_> = words(snippet).filter(|word| terms.contains(word)).collect();
        let distinct = matches.iter().collect::<HashSet<_>>().len();
        let score = distinct * 10 + matches.len().min(9);
        if distinct >= 2 && score > best.0 {
            best = (score, begin);
        }
    }
    let nearby = best.1.saturating_sub(window / 2);
    let boundary = crate::text::prefix(text, nearby).len();
    text[..boundary]
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_whitespace())
        .map_or(0, |(index, character)| index + character.len_utf8())
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_relevant_passage_after_the_default_prefix() {
        let text = format!(
            "{}A client can automatically retry an idempotent request. {}",
            " unrelated document text é ".repeat(450),
            "generic appendix. ".repeat(300)
        );
        let offset = start(
            &text,
            "When can a client automatically retry an idempotent request?",
            1200,
        );
        assert!(offset > 4_000);
        assert!(text[offset..]
            .char_indices()
            .take(1200)
            .map(|(_, c)| c)
            .collect::<String>()
            .contains("automatically retry an idempotent request"));
        assert_eq!(start(&text, "unavailable widget endpoint", 1200), 0);
    }
}
