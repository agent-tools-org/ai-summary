//! Decode SSE frames into an accumulated summary.
//! Requires provider completion evidence before reporting success.

use crate::types::Usage;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};

pub(super) fn parse_streaming_response(
    reader: impl Read,
) -> Result<(String, Option<Usage>), String> {
    let mut state = StreamState::default();
    let mut data = String::new();
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|_| "failed to read summary stream")?;
        if line.is_empty() {
            if state.consume(&data)? {
                return Ok((state.text, state.usage));
            }
            data.clear();
        } else if let Some(field) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(field.strip_prefix(' ').unwrap_or(field));
        }
    }
    if !state.finished || !data.is_empty() {
        return Err("summary stream ended before completion".into());
    }
    Ok((state.text, state.usage))
}

#[derive(Default)]
struct StreamState {
    text: String,
    usage: Option<Usage>,
    finished: bool,
}

impl StreamState {
    fn consume(&mut self, data: &str) -> Result<bool, String> {
        if data.is_empty() {
            return Ok(false);
        }
        if data == "[DONE]" {
            return Ok(true);
        }
        let event: Value =
            serde_json::from_str(data).map_err(|_| "invalid summary stream frame")?;
        if !event["error"].is_null() {
            return Err(format!("summary stream error: {}", event["error"]));
        }
        let choice = &event["choices"][0];
        if let Some(text) = choice["delta"]["content"]
            .as_str()
            .or(choice["message"]["content"].as_str())
        {
            self.text.push_str(text);
        }
        match choice["finish_reason"].as_str() {
            Some("stop") => self.finished = true,
            Some(reason) => return Err(format!("summary stopped with reason {reason}")),
            None => {}
        }
        if !event["usage"].is_null() {
            self.usage = serde_json::from_value(event["usage"].clone()).ok();
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_multiline_frames_and_usage_after_finish() {
        let input = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\r\n\r\n",
            "data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"usage\":\n",
            "data: {\"prompt_tokens\":7,\"completion_tokens\":1}}\n\n",
            "data: [DONE]\n\n",
        );
        let (text, usage) = parse_streaming_response(input.as_bytes()).unwrap();
        assert_eq!(text, "answer");
        assert_eq!(usage.unwrap().prompt_tokens, 7);
    }

    #[test]
    fn rejects_eof_after_an_unfinished_summary() {
        let input = b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
        assert!(parse_streaming_response(input.as_slice()).is_err());
    }
}
