//! Build evidence context and report source coverage for deep search.
//! Keeps structured and human-readable output consistent.

use super::{MODE, TARGET_SITES};
use crate::{
    commands::output,
    fetch::FetchBatch,
    llm::Summary,
    search::deep::{host, Discovery, Lookup},
    types::{FetchedPage, SearchResult},
    Cli,
};
use serde::Serialize;
use serde_json::json;
use std::{collections::BTreeSet, fmt::Write};

#[cfg(test)]
mod tests;

#[derive(Serialize)]
pub(super) struct Evidence<'a> {
    id: usize,
    url: &'a str,
    title: &'a str,
    snippet: &'a str,
    lookup_ids: &'a [usize],
    final_url: Option<&'a str>,
    extractor: Option<&'a str>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    #[serde(skip)]
    pub(super) page: Option<&'a FetchedPage>,
}

pub(super) fn evidence<'a>(
    discovery: &'a Discovery,
    batch: &'a FetchBatch,
    raw: bool,
) -> Vec<Evidence<'a>> {
    discovery
        .sources
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let SearchResult {
                url,
                title,
                snippet,
            } = &candidate.result;
            let page = batch.pages.iter().find(|page| page.url == *url);
            Evidence {
                id: index + 1,
                url,
                title,
                snippet,
                lookup_ids: &candidate.lookup_ids,
                final_url: page.map(|page| page.final_url.as_str()),
                extractor: page.map(|page| page.extractor),
                truncated: page.is_some_and(|page| page.truncated),
                text: page.filter(|_| raw).map(|page| page.text.as_str()),
                page,
            }
        })
        .collect()
}

pub(super) fn context(sources: &[Evidence<'_>]) -> String {
    const SOURCE_BYTES: usize = 12_000;
    let mut result = String::new();
    for source in sources {
        if let Some(page) = source.page {
            let _ = writeln!(
                result,
                "\n### Source [{}] {}\nURL: {}\n{}",
                source.id,
                source.title,
                page.final_url,
                crate::text::prefix(&page.text, SOURCE_BYTES)
            );
        }
    }
    result
}

#[derive(Serialize)]
pub(super) struct Report<'a> {
    pub(super) status: &'static str,
    queries: &'a [Lookup],
    sources: Vec<Evidence<'a>>,
    failed: &'a [crate::fetch::FetchFailure],
    fetched_sites: usize,
    cited_sites: usize,
    cited_ids: BTreeSet<usize>,
    unresolved_citations: BTreeSet<usize>,
    warnings: Vec<&'static str>,
    pub(super) error: Option<String>,
    #[serde(skip)]
    answer: Option<&'a Summary>,
}

impl<'a> Report<'a> {
    pub(super) fn new(
        discovery: &'a Discovery,
        batch: &'a FetchBatch,
        sources: Vec<Evidence<'a>>,
        answer: Option<&'a Summary>,
        error: Option<String>,
    ) -> Self {
        let fetched_sites = sites(sources.iter());
        let references = answer
            .map(|answer| citations(&answer.text))
            .unwrap_or_default();
        let (cited_ids, unresolved_citations): (BTreeSet<_>, BTreeSet<_>) =
            references.into_iter().partition(|id| {
                sources
                    .iter()
                    .any(|source| source.id == *id && source.page.is_some())
            });
        let cited_sites = sites(
            sources
                .iter()
                .filter(|source| cited_ids.contains(&source.id)),
        );
        let warnings = coverage_warnings(
            &discovery.lookups,
            fetched_sites,
            answer.map(|_| cited_sites),
            !unresolved_citations.is_empty(),
        );
        let status = if batch.pages.is_empty() || error.is_some() {
            "failed"
        } else if !warnings.is_empty() || !batch.failed.is_empty() {
            "partial"
        } else {
            "ok"
        };
        Self {
            status,
            queries: &discovery.lookups,
            sources,
            failed: &batch.failed,
            fetched_sites,
            cited_sites,
            cited_ids,
            unresolved_citations,
            warnings,
            error,
            answer,
        }
    }
}

fn coverage_warnings(
    lookups: &[Lookup],
    fetched_sites: usize,
    cited_sites: Option<usize>,
    unresolved: bool,
) -> Vec<&'static str> {
    let mut warnings = Vec::new();
    if lookups.iter().any(|lookup| lookup.error.is_some()) {
        warnings.push("Some search queries failed. See the lookup results for details.");
    }
    if fetched_sites < TARGET_SITES {
        warnings.push("Fewer than three sites yielded full-text evidence.");
    }
    if cited_sites.is_some_and(|count| count < TARGET_SITES) {
        warnings.push("The answer cites fewer than three sites.");
    }
    if unresolved {
        warnings.push("Some citations do not correspond to fetched sources.");
    }
    warnings
}

fn sites<'a, 'b: 'a>(sources: impl Iterator<Item = &'a Evidence<'b>>) -> usize {
    sources
        .filter_map(|source| host(source.final_url?))
        .collect::<BTreeSet<_>>()
        .len()
}

fn citations(answer: &str) -> BTreeSet<usize> {
    answer
        .split('[')
        .skip(1)
        .filter_map(|part| {
            let (number, _) = part.split_once(']')?;
            if number.bytes().all(|byte| byte.is_ascii_digit()) {
                return number.parse().ok();
            }
            None
        })
        .collect()
}

pub(super) fn emit(cli: &Cli, query: &str, report: &Report<'_>, duration: f64) {
    if output::structured(cli) {
        let mut fields = serde_json::to_value(report).expect("serializable deep search report");
        fields["query"] = json!(query);
        fields["mode"] = json!(MODE);
        fields["duration_secs"] = json!(duration);
        if let Some(answer) = report.answer {
            fields["summary"] = json!(answer.text);
            fields["provider"] = json!(answer.provider);
            fields["model"] = json!(answer.model);
            fields["usage"] = json!(answer.usage);
        }
        output::emit_json(fields);
        return;
    }
    emit_text(cli.raw, report);
}

fn emit_text(raw: bool, report: &Report<'_>) {
    if let Some(answer) = report.answer {
        println!("{}\n", answer.text);
    }
    for source in &report.sources {
        let url = source.final_url.unwrap_or(source.url);
        let availability = if source.page.is_none() {
            " (snippet only)"
        } else {
            ""
        };
        println!("[{}] {}{} — {}", source.id, source.title, availability, url);
        if raw {
            println!("{}", source.text.unwrap_or(source.snippet));
        }
    }
    eprintln!(
        "[ai-summary] {} lookups, {} sites fetched, {} sites cited",
        report.queries.len(),
        report.fetched_sites,
        report.cited_sites
    );
    for lookup in report.queries {
        if let Some(error) = &lookup.error {
            eprintln!("[ai-summary] {} lookup: {error}", lookup.provider);
        }
    }
    for warning in &report.warnings {
        eprintln!("[ai-summary] {warning}");
    }
    if let Some(error) = &report.error {
        eprintln!("[ai-summary] Deep search synthesis failed: {error}");
    }
}
