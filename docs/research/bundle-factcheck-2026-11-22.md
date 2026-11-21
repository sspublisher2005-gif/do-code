# Fact-check: docode_research_bundle.zip (2026-11-22)

**Purpose:** prevent contaminated claims from the bundle entering our frozen PDR or code. **Rule 1** (PDR.md:10-14) governs.

## Bundle contents (SHA-256 in MANIFEST.txt)
- 4 prior pasted-text files (contextual notes, ChatGPT/Gemini outputs — hypotheses, not facts)
- deep_research_prompt.txt (the prompt; itself correctly says "treat numeric claims as untrusted")
- deep_research_report_snapshot.md (hedged snapshot — GOOD, aligns with our PDR)

## Verdicts (fetched 2026-11-22)

| Claim | Verdict | Action |
|---|---|---|
| "Bastion — OpenAI's safety kernel, Apache-2.0, use directly" | FALSE: `michaelwinczuk/bastion`, 2★, README MIT OR Apache-2.0, last push 2026-04-09, 17 integration tests. NOT OpenAI. | REJECT as dependency. We already have primitives (contract.rs, patches.rs checkpoints, evidence.rs, audit.rs, loopctl.rs). Reimplement specs only if needed. |
| "Doc2Harness (Ge et al.) HARNESS-1 spec, 9 flags" | NOT FOUND (GitHub API search: 0 results). | REJECT — do not build to it. |
| "IETF agent-receipts / Evidence Qualification Receipt draft" | NOT VERIFIED. IETF SCITT draft is real (draft-ietf-scitt-architecture-10, Appendix B: Receipts) but about supply-chain transparency, not coding-agent receipts. Community: `agent-receipts/obsigna` 23★ Apache-2.0 Go; `inchwormz/agent-receipts` 2★ MIT Rust; `effectustasi/agent-receipts` 1★ MIT. | Never claim EQR/SCITT compliance. Keep our own SHA-256 evidence manifests. |
| "sandboxrs-windows crate" | REAL: crates.io v0.1.1, published 2026-08-11, MIT, 175 downloads, by TarunKurella, Rust 1.90, 5,774 code lines. | Do NOT replace our driver. Study only; too new/unproven for a security boundary. |
| "jcode (duymap, 2024), picocli" | FALSE: real is `1jehuang/jcode`, Rust, MIT, created 2026-01-05, ~22k★. picocli is Java. | Correct in all notes. |
| "Cline (arensito)", "OpenCode (AOSP)", "Hermes (TeamADAPT)", "Roo (Team Adapt)" | Wrong owners. Real: cline/cline, anomalyco/opencode, NousResearch/hermes-agent, RooCodeInc (archived/sunset 2026-05-15). | Correct. |
| "Terminal-Bench 4.1 / SWE-bench archived at 97% / 77% solve rate / GPT-6.2 / Opus 5.5 / Grok 4.7 / jcode 19.1ms / Claude 386MB" | UNVERIFIED blog/vendor claims (morphllm.com, explainx.ai). Terminal-Bench itself real (harbor-framework/terminal-bench, Apache-2.0, 1.1k★; 2.1 repo exists). | Never quote numbers as fact. Use Terminal-Bench later as an optional eval harness (P2), not a gate. |

## Adopted from the hedged snapshot (already in our PDR)
Receipt schema: contract version/hash, before/after workspace or patch hashes, tool/command IDs, exact argv, exit code, output/log digests, check names + results, approvals, timestamps, provider/model IDs, PASS/FAIL/BLOCKED/NEEDS_HUMAN, replay path. Source: deep_research_report_snapshot.md §4 "Evidence receipts" (hedged). Our `docs/evals/README.md` already specifies this.

## Rejected (Rule 1: adds risk or false claims)
Bastion dependency · Doc2Harness · IETF EQR compliance · sandboxrs-windows as driver · all quoted benchmarks/stars · fabricated model names.

## Plan impact
NONE. Frozen PDR (6017bef) and research deliverables (c36b9d7) stand. Next work remains M1: qualify Windows sandbox (26-case matrix, evidence-driven is_qualified) → execution receipts → bounded repair loop. Windows-only; no Linux/macOS until pass.

## Access gaps
Reddit/X/YouTube/video transcripts unavailable during this fact-check (body tools were replaced mid-session; websearch HTTP 402). Those platforms were covered in earlier research rounds (docs/research/SOURCES.md). No claims added from them here.
