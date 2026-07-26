//! Output cleanup helpers for model responses.
//! Exports: strip_thinking().
//! Deps: std string handling only.

pub fn strip_thinking(s: &str) -> String {
    let s = s.trim();
    if let Some(index) = s.find("</think>") {
        return s[index + 8..].trim().to_string();
    }
    let lines: Vec<&str> = s.lines().collect();
    let starters = [
        "Thinking Process",
        "**Thinking",
        "Let me think",
        "I'll analyze",
        "Let me analyze",
        "First, let me",
    ];
    let mut thinking = false;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if index < 3 && starters.iter().any(|starter| trimmed.starts_with(starter)) {
            thinking = true;
            continue;
        }
        if thinking && is_summary_start(trimmed, &starters) {
            return lines[index..].join("\n").trim().to_string();
        }
    }
    s.to_string()
}

fn is_summary_start(trimmed: &str, starters: &[&str]) -> bool {
    trimmed.starts_with("## ")
        || trimmed.starts_with("# ")
        || trimmed == "---"
        || trimmed.starts_with("- **")
        || trimmed.starts_with("- [")
        || trimmed.starts_with("1.")
        || trimmed.starts_with("Here")
        || trimmed.starts_with("Based on")
        || (trimmed.starts_with("**")
            && !starters.iter().any(|starter| trimmed.starts_with(starter)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_thinking_after_close_tag() {
        let text = "<think>private notes</think>\nFinal answer";
        assert_eq!(strip_thinking(text), "Final answer");
    }
}
