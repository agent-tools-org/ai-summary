//! Exercise report provenance using acquired and snippet-only sources.
//! Verifies redirects and invalid citation IDs do not inflate coverage.

use super::*;
use crate::{fetch::FetchFailure, llm::Provider, search::deep::Candidate};

#[test]
fn answers_reference_fetched_pages_and_report_redirects_and_gaps() {
    let urls = [
        "https://a.example/one",
        "https://b.example/two",
        "https://c.example/missing",
    ];
    let discovery = Discovery {
        lookups: Vec::new(),
        sources: urls
            .into_iter()
            .map(|url| Candidate {
                result: SearchResult {
                    url: url.into(),
                    title: "Evidence".into(),
                    snippet: "Snippet".into(),
                },
                lookup_ids: vec![0],
            })
            .collect(),
    };
    let batch = FetchBatch {
        pages: urls[..2]
            .iter()
            .map(|url| FetchedPage {
                url: (*url).into(),
                final_url: "https://same.example/evidence".into(),
                extractor: "text",
                media_type: "text/plain".into(),
                truncated: false,
                text: "Acquired source text".into(),
            })
            .collect(),
        failed: vec![FetchFailure {
            url: urls[2].into(),
            reason: "unreachable".into(),
        }],
    };
    let sources = evidence(&discovery, &batch, false);
    let content = context(&sources);
    assert!(content.contains("Source [2]"));
    assert!(!content.contains("Source [3]"));
    let answer = Summary::new(
        "Good [1] [2], rejected [3] [9].",
        None,
        Provider::Http,
        "fixture",
    )
    .unwrap();
    let report = Report::new(&discovery, &batch, sources, Some(&answer), None);
    assert_eq!(report.status, "partial");
    assert_eq!((report.fetched_sites, report.cited_sites), (1, 1));
    assert_eq!(report.cited_ids, BTreeSet::from([1, 2]));
    assert_eq!(report.unresolved_citations, BTreeSet::from([3, 9]));
}
