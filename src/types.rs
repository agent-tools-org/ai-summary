// Shared data types used across search, fetch, LLM, and command modules.
// Exports: SearchResult, FetchedPage, ChatResponse, Choice, MessageContent, Usage, SummarizeResult.
// Deps: serde::Deserialize for API response decoding.

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct FetchedPage {
    pub url: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Deserialize)]
pub struct Choice {
    pub message: MessageContent,
}

#[derive(Deserialize)]
pub struct MessageContent {
    pub content: String,
}

#[derive(Deserialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

pub struct SummarizeResult {
    pub text: String,
    pub usage: Option<Usage>,
    pub raw_chars: u64,
    pub summary_chars: u64,
}
