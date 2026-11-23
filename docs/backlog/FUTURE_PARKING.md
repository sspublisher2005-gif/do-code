## Added from cross-platform market research (2026-11-23)

| Idea | Why parked | Unpark condition |
|---|---|---|
| Agent-native JSON/MCP/structured-log surfaces | No tool-call loop yet; CLI-only today | Tool-call loop (Phase 1) ships and is used daily |
| Signed evidence bundles (.docode-proof) + offline verifier | Evidence ledger exists but no export/sign format | 3 real users request portable proof |
| Open Evidence Standard (MCP-for-proof spec) | Needs a stable schema first | Evidence bundle format stabilises + 1 external consumer |
| Non-coder guided mode (plain-language risk/test summary) | CLI-only today; needs UX research | V1.0 CLI passes AND user requests non-coder surface |
| Plain-language error explanations (--explain) | Error messages are engineer-facing | Non-coder guided mode starts |
| Screen reader / a11y / i18n | No UI surface yet | TUI or desktop surface exists |
| Catwalk-style model catalog (auto-update, quality opinions) | Single model config today | 3+ providers in active use |
| Offline-first mode (air-gapped, local models only) | Ollama supported but no offline guarantee | User requests air-gapped operation |
| Cost governor (hard spend cap from provider-reported usage) | No cost tracking today | Usage reporting wired into provider layer |
| Prompt caching (Anthropic/OpenAI cache control) | No caching today | Provider layer supports cache_control |
| Context compaction / summarisation | Fixed context pack, no compaction | Sessions exceed 80% of context window in practice |
| Semantic retrieval / embeddings | Substring ranking today | Outline + grep insufficient on large repos |
| TUI (ratatui) | CLI only | CLI proven loved by 10+ users |
| Subagents / parallel agents | Sequential only | Agent loop stable + demand |
| Eval suite on real tasks (Terminal-Bench adapter) | No eval today | Tool loop ships |
| Doc explain / verification layer | Evidence exists, no explain command | Evidence bundle format stabilises |
| Plain-language error explanations | None | Non-coder demand signal |
| Model quality opinions / Catwalk catalog | None | 3+ providers in use |
| Spend ceiling from provider-reported usage | None | Cost tracking ships |
| AGENTS.md / project instructions support | Partial (DOCODE.md) | Tool loop ships |
| Git history / blame context | None | Context engine ships |
| .gitignore / binary / large-file handling | None | Context engine ships |
| Network egress control inside sandbox | Network denied by default | Sandbox qualified |
| Timeout / OOM / fork-bomb protection | Job Object limits | Sandbox qualified |
| Build/lint/format integration | None | Tool loop ships |
| Coverage measurement | None | Test runner integration ships |
| Static analysis / SAST hooks | None | Test runner integration ships |
| Fuzzing / property tests | None | Test runner integration ships |
| Benchmark harness on real repos | None | Eval suite ships |
| Semantic retrieval / embeddings | Substring ranking | Outline loses on cross-module renames |
| Dependency graph awareness | None | Tree-sitter outline ships |
| Prompt caching | None | Provider layer supports cache_control |
| Model capability detection | None | 3+ providers in use |
| Per-tool permission policy allow/ask/deny | Policy table only | Tool loop ships |
| Secret redaction from logs/context | Redaction exists, not wired to tool output | Tool loop ships |
| Prompt-injection defence | None | Tool loop ships |
| Supply-chain protection | None | First public release |
| Session list / switch / resume | None | Session persistence ships |
| Keyboard-driven approve/reject | None | TUI ships |
| Headless / CI mode | Partial | Tool loop ships |
| IDE integration | None | CLI loved + demand |
| Web/desktop surface | None | CLI loved + demand |
| --json machine-readable output | None | Headless mode ships |
| Shell completion, man page | None | First public release |
| MCP client tools + resources | None | Tool loop ships |
| Hooks pre/post tool, pre/post commit | None | Tool loop ships |
| Skills / reusable prompt packs | DOCODE.md only | Hooks ship |
| Plugin system | None | Signed plugin format + 3 plugins |
| Custom commands | None | Skills ship |
| LSP client | None | Tree-sitter insufficient |
| Failure taxonomy + tracked fix rate | None | 10 real failures collected |
| Dogfooding | Not happening | Tool loop ships |
| Telemetry opt-in | None | Privacy policy written |
| Performance budget | None | First public release |
| Install path on clean machine | None | First public release |
| Docs site / quickstart | README only | V1.0 gate passes |
| Release cadence + changelog + semver | ~50 commits, no releases | V1.0 gate passes |
| Demo 30-second GIF | None | First public release |
| Issue templates, contribution guide, CI badge | Partial (CI exists) | First external contributor |
| Public roadmap | None | V1.0 gate passes |
| Positioning | Unclear | V1.0 gate passes |
| Community Discord/Discussions | None | 100+ users |
| Auto-update | None | First public release |
| Signed evidence bundles | None | Evidence bundle format stabilises |
| Open Evidence Standard | None | Signed bundles ship |
| Offline-first mode | Ollama supported | User requests air-gapped |
| Cost governor | None | Cost tracking ships |
| Resume sessions | Inspection only | Session persistence ships |
| Context compaction | None | Sessions exceed 80% context |
| TUI | None | CLI proven loved |
| Subagents / parallel | None | Agent loop stable + demand |
| Eval suite on real tasks | None | Tool loop ships |
| Repo map / tree-sitter | Brace outline exists | Outline loses on cross-module renames |
| Doc explain / verification layer | Evidence exists | Evidence bundle format stabilises |
| Guided mode for non-coders | None | Non-coder demand signal |
| Plain-language error explanations | None | Non-coder demand signal |
| Screen reader / a11y / i18n | None | TUI or desktop ships |
| Model quality opinions / Catwalk catalog | None | 3+ providers in use |
| Spend ceiling from provider-reported usage | None | Cost tracking ships |
| AGENTS.md / project instructions support | Partial (DOCODE.md) | Tool loop ships |
| Git history / blame context | None | Context engine ships |
| .gitignore / binary / large-file handling | None | Context engine ships |
| Network egress control inside sandbox | Network denied by default | Sandbox qualified |
| Timeout / OOM / fork-bomb protection | Job Object limits | Sandbox qualified |
| Build/lint/format integration | None | Tool loop ships |
| Coverage measurement | None | Test runner integration ships |
| Static analysis / SAST hooks | None | Test runner integration ships |
| Fuzzing / property tests | None | Test runner integration ships |
| Benchmark harness on real repos | None | Eval suite ships |
| Semantic retrieval / embeddings | Substring ranking | Outline loses on cross-module renames |
| Dependency graph awareness | None | Tree-sitter outline ships |
| Prompt caching | None | Provider layer supports cache_control |
| Model capability detection | None | 3+ providers in use |
| Per-tool permission policy allow/ask/deny | Policy table only | Tool loop ships |
| Secret redaction from logs/context | Redaction exists, not wired to tool output | Tool loop ships |
| Prompt-injection defence | None | Tool loop ships |
| Supply-chain protection | None | First public release |
| Session list / switch / resume | None | Session persistence ships |
| Keyboard-driven approve/reject | None | TUI ships |
| Headless / CI mode | Partial | Tool loop ships |
| IDE integration | None | CLI loved + demand |
| Web/desktop surface | None | CLI loved + demand |
| --json machine-readable output | None | Headless mode ships |
| Shell completion, man page | None | First public release |
| MCP client tools + resources | None | Tool loop ships |
| Hooks pre/post tool, pre/post commit | None | Tool loop ships |
| Skills / reusable prompt packs | DOCODE.md only | Hooks ship |
| Plugin system | None | Signed plugin format + 3 plugins |
| Custom commands | None | Skills ship |
| LSP client | None | Tree-sitter insufficient |
| Failure taxonomy + tracked fix rate | None | 10 real failures collected |
| Dogfooding | Not happening | Tool loop ships |
| Telemetry opt-in | None | Privacy policy written |
| Performance budget | None | First public release |
| Install path on clean machine | None | First public release |
| Docs site / quickstart | README only | V1.0 gate passes |
| Release cadence + changelog + semver | ~50 commits, no releases | V1.0 gate passes |
| Demo 30-second GIF | None | First public release |
| Issue templates, contribution guide, CI badge | Partial (CI exists) | First external contributor |
| Public roadmap | None | V1.0 gate passes |
| Positioning | Unclear | V1.0 gate passes |
| Community Discord/Discussions | None | 100+ users |
| Auto-update | None | First public release |
| Spend ceiling from provider-reported usage | None | Cost tracking ships |

## How to use this file
When a new idea arrives: (1) check this file; (2) if parked and condition unmet -> do not build, record the request in DECISION_LOG; (3) if condition met -> promote to FEATURE_LEDGER with status FUTURE or PLANNED; (4) if rejected forever -> leave here with reason.
