# ai-summary 3.0 validation

The 3.0 CLI keeps a single Rust package. The work covers summary outcomes and configuration, document acquisition and caching, provider adapters, local compression and wrapper behavior, Claude integration, and local persistence.

| Contract | Validation |
| --- | --- |
| HTTP summary failure, output, and credentials | Loopback CLI fixtures check nonzero failures, redaction, endpoint isolation, and recovery after a failed request. |
| Documents and acquisition | Local text, HTML, PDF, and DOCX fixtures check extraction. Concurrent HTTP fixtures check input order and partial failure. |
| Cache and terminal | A real pseudo-terminal checks cached output, completed events, and rejection of an incomplete response. Fixtures check cache bypass, expiry, source changes, and model changes. |
| Provider adapters | Synthetic GitHub, Repomix, and OpenCode executables check argv, failures, and large stdin transport. Loopback Cloudflare fixtures check cursor pagination and partial terminal states. Gemini fixtures check model and source attribution. |
| Local utilities and hooks | CLI fixtures check UTF-8 limits, exact subprocess arguments, stream and exit preservation, and Claude install/reinstall/uninstall with third-party settings. Shell smoke checks normal permission behavior and separate sessions. |
| Persistence | Fixtures check 12 concurrent writers, migration of unversioned statistics, and keeping malformed files intact. |

The Linux suite runs 111 Rust tests. Clippy with warnings denied, the locked optimized build, and a locked Rust 1.89 typecheck passed. Five added CLI regression cases failed against pre-change source and passed after the change.

The local corpus returned four pages and 571 extracted bytes in 21 ms on Linux and 10 ms on Apple Silicon during validation. These single-run smoke measurements use local fixtures. They do not measure public-network latency or answer quality.

Live hosted API authentication, externally rendered pages, and Intel macOS were outside the measured scope.

The [3.1 dogfood notes](investigation-dogfood-2026-11-19.md) record live provider behavior, deep-search source coverage, and extractor checks.
