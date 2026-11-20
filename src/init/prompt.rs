//! Edit only the marker-delimited ai-summary instructions.
//! Validates marker ownership before any file is written.

use std::{fs, io::ErrorKind, ops::Range, path::Path};

const PROMPT: &str = include_str!("../../prompts/claude-code.md");
const START: &str = "<!-- ai-summary:start -->";
const END: &str = "<!-- ai-summary:end -->";

pub(super) fn plan(path: &Path, uninstall: bool) -> Result<Option<String>, String> {
    let mut text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("cannot read Claude instructions: {error}")),
    };
    let owned = owned_range(&text)?;
    if uninstall {
        if let Some(owned) = owned {
            text.replace_range(owned, "");
            return Ok(Some(text));
        }
        return Ok(None);
    }
    let block = format!("{START}\n{}\n{END}\n", PROMPT.trim_end());
    if let Some(owned) = owned {
        if text[owned.clone()] == block {
            return Ok(None);
        }
        text.replace_range(owned, &block);
    } else {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if !text.is_empty() && !text.ends_with("\n\n") {
            text.push('\n');
        }
        text.push_str(&block);
    }
    Ok(Some(text))
}

fn owned_range(text: &str) -> Result<Option<Range<usize>>, String> {
    let mut start = None;
    let mut result = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        match line.trim_end_matches(['\r', '\n']) {
            START if start.is_none() && result.is_none() => start = Some(offset),
            END if start.is_some() => {
                result = start.take().map(|start| start..offset + line.len());
            }
            START | END => {
                return Err("Claude instructions contain malformed ai-summary markers".into())
            }
            _ => {}
        }
        offset += line.len();
    }
    if start.is_some() {
        return Err("Claude instructions contain an unclosed ai-summary marker".into());
    }
    Ok(result)
}
