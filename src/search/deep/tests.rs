//! Verify deep search lookup planning and source coverage with synthetic hits.
//! Search providers stay offline in these tests.

use super::*;

fn hit(url: &str) -> SearchResult {
    SearchResult {
        url: url.into(),
        title: "Evidence".into(),
        snippet: "Snippet".into(),
    }
}

#[test]
fn runs_all_angles_deduplicates_tracking_links_and_prioritizes_sites() {
    let discovery = research("test", true, false, |provider, query| {
        if provider == "brave" {
            return Ok(vec![hit("https://c.example/path")]);
        }
        if query.contains("specification") {
            return Ok(vec![
                hit("https://a.example/doc?utm_source=search"),
                hit("https://b.example/path"),
            ]);
        }
        Ok(vec![
            hit("https://a.example/doc#section"),
            hit("https://a.example/second"),
        ])
    });
    assert_eq!(discovery.lookups.len(), 4);
    let sources: Vec<_> = discovery
        .sources
        .iter()
        .map(|item| item.result.url.as_str())
        .collect();
    assert_eq!(
        sources,
        [
            "https://a.example/doc#section",
            "https://c.example/path",
            "https://b.example/path",
            "https://a.example/second"
        ]
    );
    assert_eq!(discovery.sources[0].lookup_ids, vec![0, 1, 2]);
}

#[test]
fn broadens_search_when_initial_results_have_one_site() {
    let discovery = research("test", false, false, |_, query| {
        if query.contains("-site:a.example") {
            Ok(vec![hit("https://b.example/new")])
        } else {
            Ok(vec![hit("https://a.example/doc")])
        }
    });
    assert_eq!(discovery.lookups.len(), 4);
    assert_eq!(discovery.sources.len(), 2);
}

#[test]
fn a_missing_primary_query_recovers_with_grounding_when_enabled() {
    let discovery = research("When is retry safe?", false, true, |provider, query| {
        if provider == "gemini_google" {
            assert!(query.contains("primary documents"));
            return Ok(vec![hit("https://primary.example/spec")]);
        }
        assert!(query.starts_with("When is retry safe?") || query.starts_with("retry safe "));
        Err("provider requested verification".into())
    });
    assert_eq!(discovery.lookups.len(), 4);
    assert_eq!(discovery.lookups[3].provider, "gemini_google");
    assert_eq!(
        discovery.sources[0].result.url,
        "https://primary.example/spec"
    );
}
