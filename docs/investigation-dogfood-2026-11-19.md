KB consulted: `kb dogfood CLI fetch provider recovery search output`.

# ai-summary 3.0 and 3.1 dogfood

The installed Apple Silicon binary was used to research HTTP retries from public sources, read a public GitHub repository, and summarize a short public paragraph. The configured Gemini and OpenRouter routes each received one request with public input. Tests used isolated state. The OpenCode run used the installed executable with an isolated home and the CLI’s default free model identifier.

| Workflow | Observation |
| --- | --- |
| Direct URL batch | Two of four sources returned usable content in 0.55 s. RFC 9110 HTML and plain text failed at the 200,000-byte body limit. The reqwest docs output began with navigation and package metadata. |
| DuckDuckGo raw search | Found RFC 9110 as the first result in 2.89 s. Its body failed at the same limit. Another source returned HTTP 403. |
| GitHub file read | `github cli/cli README.md` returned the requested file in 1.12 s. |
| Gemini search | Returned a relevant answer in 16.91 s with six sources and 15 citation spans. JSON was 10,466 bytes. A bullet introduced a summary as a client `SHOULD` requirement, whereas the quoted RFC passage describes when a retry can be performed. [RFC 9110 section 9.2.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.2.2) |
| Installed Defuddle 0.13.0 | The current stdin invocation exits 1 and asks for a source. An explicit `-` is treated as a filename. The same HTML succeeds with a file operand. |
| Installed agent-browser 0.21.4 | Explicit browser fetch exits 2. The same child command reports `Unknown command: --no-auto-connect`. The CLI only reports that the page failed to open. |
| Summary providers | The configured OpenRouter request returned HTTP 402 for insufficient credits. The configured loopback endpoint refused a TCP connection. The free OpenCode smoke exceeded its 30 s observation deadline in the isolated environment. After the smoke runner killed the parent, the identified child group was still running and was stopped. |
| Repository pack | The optional Repomix executable was absent. The command returned an installation hint along with a generic I/O error. |

These timings are single observations on the same machine. The OpenCode result does not establish whether it would eventually respond under the application's longer deadline, or whether the normal user profile would behave identically.

## Findings carried into 3.1

1. Restore extractor compatibility. Exercise Defuddle 0.13.0 with a private temporary file when stdin is unsupported. Detect the browser CLI's isolation flags before launch. Preserve isolated browsing and return a version-specific diagnostic if it cannot be guaranteed. There should be no silent extraction fallback that hides the actual child failure.
2. Make summary route readiness actionable. Expose dependency versions and configured route health before a long request. Distinguish a credit failure, an unreachable local service, a slow child, and a missing optional executable. Verify cancellation of the started subprocess tree on both SIGINT and SIGTERM without signaling unrelated processes.
3. Fetch primary sources under explicit limits. Support bounded extraction from long HTML and text and prioritize the named section when a fragment is supplied. Preserve a body truncation indicator and citation URL. Include a fixture with the requested section beyond 200 KB and a corpus case based on RFC 9110. Keep the context byte ceiling and ordered failure reporting.
4. Improve grounded output for agents. Keep a compact source list and reference it from claim spans rather than duplicating long URLs. Check that answer wording preserves the normative strength in the source. Capture output size, the presence of the primary source, and citation integrity on the same small query corpus.

## 3.1.0 follow-up

- The Apple Silicon build passed the isolated CLI smoke test and fetched the anchored RFC 9110 section. The installed versions of Defuddle and agent-browser returned extracted text.
- The RFC retry question took 22.8 seconds in deep mode. The CLI fetched nine sites and cited five, including both official RFC 9110 document formats. Every inline source number resolved to a fetched page. The report retained DuckDuckGo's human verification responses and showed the successful Google grounding lookup.
- A raw deep run reached five sites earlier in the session. A later run reported DuckDuckGo's human verification and returned exit 2 with a structured report. Brave was unconfigured.
- Ordinary Gemini grounded output measured 3,936 bytes for the previous 10,466-byte sample question. These are separate model responses. Version 2 lists each unique source URL once, and every citation index resolved in the follow-up sample.
- The Chinese Git workflow question initially fetched evidence and failed at synthesis. The revised prompt and output budget completed a Chinese answer in 53.7 seconds. It fetched eight sites and cited seven. The completed run contained secondary sources and a community discussion. It did not cite the official Git manual.
