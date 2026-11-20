//! Run a bounded set of complementary search queries and diversify results.
//! Tracks which lookup discovered each source and preserves provider failures.

use super::{search_brave, search_ddg, search_gemini};
use crate::config::Config;
use crate::types::SearchResult;
use reqwest::{blocking::Client, Url};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const PER_QUERY: usize = 6;
const MAX_SOURCES: usize = 10;
const DISTINCT_TARGET: usize = 3;

#[derive(Serialize)]
pub struct Lookup {
    pub query: String,
    pub provider: &'static str,
    pub hits: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct Candidate {
    pub result: SearchResult,
    pub lookup_ids: Vec<usize>,
}

pub struct Discovery {
    pub lookups: Vec<Lookup>,
    pub sources: Vec<Candidate>,
}

pub fn discover(client: &Client, question: &str, cfg: &Config, allow_grounding: bool) -> Discovery {
    research(
        question,
        !cfg.brave_api_key.is_empty(),
        allow_grounding && !cfg.gemini_api_key.is_empty(),
        |provider, query| match provider {
            "brave" => search_brave(query, PER_QUERY, &cfg.brave_api_key),
            "gemini_google" => grounded_sources(cfg, query),
            _ => search_ddg(client, query, PER_QUERY),
        },
    )
}

fn grounded_sources(cfg: &Config, query: &str) -> Result<Vec<SearchResult>, String> {
    let answer = search_gemini(query, &cfg.gemini_api_key, &cfg.gemini_model)?;
    Ok(answer
        .sources
        .into_iter()
        .map(|source| SearchResult {
            url: source.url,
            title: source.title,
            snippet: String::new(),
        })
        .collect())
}

fn research<F>(question: &str, has_brave: bool, has_grounding: bool, search: F) -> Discovery
where
    F: Fn(&str, &str) -> Result<Vec<SearchResult>, String> + Sync,
{
    let queries = plan(question);
    let mut jobs: Vec<_> = queries.into_iter().map(|query| ("ddg", query)).collect();
    if has_brave {
        jobs.push(("brave", jobs[0].1.clone()));
    }
    let mut batches = std::thread::scope(|scope| {
        let runs: Vec<_> = jobs
            .into_iter()
            .map(|(provider, query)| {
                let search = &search;
                scope.spawn(move || execute(provider, query, search))
            })
            .collect();
        runs.into_iter()
            .map(|handle| handle.join().expect("search worker"))
            .collect::<Vec<_>>()
    });
    if let Some((provider, query)) = follow_up(question, &batches, has_brave, has_grounding) {
        batches.push(execute(provider, query, &search));
    }
    combine(batches)
}

fn follow_up(
    question: &str,
    batches: &[(Lookup, Vec<SearchResult>)],
    has_brave: bool,
    has_grounding: bool,
) -> Option<(&'static str, String)> {
    let mut hosts = batches
        .iter()
        .flat_map(|(_, hits)| hits)
        .filter_map(|hit| host(&hit.url));
    let first_host = hosts.next();
    let distinct = first_host
        .iter()
        .cloned()
        .chain(hosts)
        .collect::<HashSet<_>>()
        .len();
    let needs_primary = batches[1].1.is_empty();
    if !needs_primary && distinct >= DISTINCT_TARGET {
        return None;
    }
    if has_grounding {
        let query = format!(
            "{}\nFind links to the primary documents and their relevant sections, plus independent sources that corroborate or qualify the claims.",
            crate::text::prefix(question, 600)
        );
        return Some(("gemini_google", query));
    }
    let keywords = keywords(question);
    let query = match first_host {
        Some(host) if distinct < DISTINCT_TARGET => format!("{keywords} -site:{host}"),
        _ => keywords,
    };
    let provider = if has_brave { "brave" } else { "ddg" };
    let already_ran = batches
        .iter()
        .any(|(lookup, _)| lookup.query == query && lookup.provider == provider);
    (!already_ran).then_some((provider, query))
}

fn plan(question: &str) -> [String; 3] {
    let original = crate::text::prefix(question.trim(), 600);
    let topic = keywords(original);
    let chinese = topic
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character));
    let (primary, independent) = if chinese {
        ("官方文档", "独立分析 局限")
    } else {
        ("specification", "independent analysis limitations")
    };
    [
        original.into(),
        format!("{topic} {primary}"),
        format!("{topic} {independent}"),
    ]
}

fn keywords(question: &str) -> String {
    const QUESTION_WORDS: &[&str] = &[
        "a", "an", "are", "can", "do", "does", "how", "is", "should", "the", "under", "what",
        "when", "where", "which", "why", "would",
    ];
    let terms = crate::text::prefix(question.trim(), 600)
        .split_whitespace()
        .map(|word| word.trim_matches([',', '.', '?', '!', '"', '\'', '，', '。', '？', '！']))
        .filter(|word| {
            !word.is_empty() && !QUESTION_WORDS.contains(&word.to_ascii_lowercase().as_str())
        })
        .take(20)
        .collect::<Vec<_>>();
    if terms.is_empty() {
        crate::text::prefix(question, 600).to_owned()
    } else {
        terms.join(" ")
    }
}

fn execute(
    provider: &'static str,
    query: String,
    search: &impl Fn(&str, &str) -> Result<Vec<SearchResult>, String>,
) -> (Lookup, Vec<SearchResult>) {
    let (hits, error) = match search(provider, &query) {
        Ok(hits) => (hits, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    (
        Lookup {
            query,
            provider,
            hits: hits.len(),
            error,
        },
        hits,
    )
}

fn combine(batches: Vec<(Lookup, Vec<SearchResult>)>) -> Discovery {
    let mut indices = HashMap::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    for rank in 0..MAX_SOURCES {
        for (lookup, (_, hits)) in batches.iter().enumerate() {
            let Some(hit) = hits.get(rank) else { continue };
            let Some(key) = identity(&hit.url) else {
                continue;
            };
            let index = *indices.entry(key).or_insert(candidates.len());
            if index == candidates.len() {
                candidates.push(Candidate {
                    result: hit.clone(),
                    lookup_ids: vec![lookup],
                });
            } else if !candidates[index].lookup_ids.contains(&lookup) {
                candidates[index].lookup_ids.push(lookup);
            }
        }
    }
    let mut used = HashSet::new();
    let (mut first, rest): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .partition(|candidate| host(&candidate.result.url).is_some_and(|host| used.insert(host)));
    first.extend(rest);
    first.truncate(MAX_SOURCES);
    Discovery {
        lookups: batches.into_iter().map(|(lookup, _)| lookup).collect(),
        sources: first,
    }
}

fn identity(value: &str) -> Option<String> {
    let mut url = crate::fetch::http::validate_url(value).ok()?;
    let host = url.host_str()?;
    if (host.ends_with("bing.com") && url.path() == "/aclick")
        || (host.ends_with("duckduckgo.com") && url.path() == "/y.js")
    {
        return None;
    }
    let query: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| !key.starts_with("utm_") && !matches!(key.as_ref(), "fbclid" | "gclid"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_fragment(None);
    url.set_query(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Some(url.into())
}

pub fn host(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    let host = url.host_str()?;
    Some(host.strip_prefix("www.").unwrap_or(host).to_owned())
}

#[cfg(test)]
mod tests;
