//! Apply byte limits to UTF-8 text without splitting a code point.
//! Used by context budgets, compression, and report labels.

pub fn prefix(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
