# Forge CLI — Feature parity ledger (Rust vs TypeScript reference)

**Reviewed:** 2026-10-26  
**Product:** Forge CLI (`forge`)  
**Rust target:** `0.7.0-rs.1`, current packaged source tree  
**Reference:** `Forge_CLI_TS_0.6.0-ts.2_MemoryDecision_Merge_WIP.zip`, expanded in `02_TYPESCRIPT_REFERENCE/LEGACY_EXPANDED_REFERENCE/`  
**Review type:** source and CLI-surface inspection + runtime evidence available in this handoff; not an independent Rust compilation.

## Status vocabulary

- **Verified on Linux:** executed against the supplied Linux x86-64 binary in this handoff environment and passed the stated test.
- **Implemented, Rust tests reported passing:** present in Rust source and covered by the supplied Claude build logs; Cargo was not rerun by the independent checker.
- **Implemented, not fully verified:** source exists, but coverage/platform behavior is incomplete or unavailable.
- **Partial / parity gap:** a narrower implementation or different user workflow than the TypeScript reference.
- **Not implemented / release blocker:** must remain disabled or fail closed until built and tested.

A module name or unit test is not by itself proof of end-user feature parity. The rows below distinguish code presence, test evidence, and missing product behavior.

## CLI command parity

| TypeScript command / behavior | Rust equivalent | Evidence available | Status / remaining gap |
|---|---|---|---|
| Help, version | Clap `forge --help`, `forge --version` | Linux binary launched; version/help passed; black-box suite passed | **Verified on Linux.** Help surface differs and should be parity-tested by scripted snapshots. |
| `init` | `forge init` | Black-box tests and direct probe passed; creates config, `FORGE.md`, local state; no network claimed | **Verified on Linux** for init, no-overwrite/idempotency and standard permissions. Windows ACLs remain unresolved. |
| `doctor` | `forge doctor` | Black-box test and direct probe passed | **Verified on Linux** for honest warnings. It explicitly says sandbox is not implemented. |
| `config show` | `forge config show` | Black-box test and direct probe passed; JSON parses in harness | **Verified on Linux** for current schema/defaults. Schema breadth is narrower than TS and needs fixture-by-fixture comparison. |
| `model test` | `forge model test` | Offline no-request and missing-consent checks pass; provider unit tests appear in supplied Cargo log | **Partially verified.** Real provider services are not called; local adapter contract tests are reported in Cargo logs. |
| `files list/read/search` | `forge files list/read/search` | Black-box suite verifies no write, traversal denial, sensitive paths, redaction, Unicode/binary/size guards, symlink and hardlink behavior on Linux | **Verified on Linux for tested cases.** Windows handle code and ACL behavior still need actual Windows CI. |
| `audit` | `forge audit --tail N` | Direct command output checked; black-box suite tests ordering, redaction and bounds | **Verified on Linux for covered cases.** Crash-recovery/multiprocess and OS permission cases still need extended testing. |
| `index` | `forge index` | Command and context module exist in Rust source; no dedicated independent black-box test in current 24-check run | **Implemented, not fully verified.** `.gitignore` semantics, freshness, large-repo performance and ranking remain partial. |
| `context` | `forge context <task>` | Source contains bounded context-pack assembly; context prioritization has supplied unit coverage | **Implemented, not fully verified.** Stale-index rebuild, prompt-injection containment, and direct end-to-end use are incomplete. |
| `agents [list]` | `forge agents [list]` | Source command exists; roles used in fixture E2E | **Partial.** Current runtime roles are sequential perspectives from one configured model, not independent agents; DevOps prompt/execution is blocked. |
| `plan` | `forge plan <task> --live --approve-context-sharing` | Command/source path exists; no dedicated separate `plan` run in independent black-box checks | **Implemented, not fully verified.** Need task-to-plan fixture E2E and provider variants. |
| `run` | `forge run <task> --live --approve-context-sharing` | Loopback fixture E2E ran six-role proposal workflow; local-only request path; no external API key/service | **Verified on Linux for workflow plumbing.** Real model quality/provider coverage is not established. |
| `mission list/show` | `forge mission list/show` plus `messages` | Empty mission list direct probe passed; source supports show/messages | **Partial verification.** Mission persistence unit tests reported passing; mission corruption, transcript tamper and recovery should stay in release tests. |
| `apply` | `forge apply <mission> [--approve]` | E2E verified preview makes no changes and approved application changes fixture file | **Verified on Linux for existing text-file replacement.** File create/delete/rename, binary and complete unified diff semantics are missing. |
| `verify` | `forge verify <mission>` | E2E verifies it refuses success because project tests were not actually executed | **Fail-closed behavior verified.** Completion/evidence helper types are not fully wired into live mission state; real verified test evidence cannot yet be generated by a sandbox. |
| `resume` | `forge resume <mission>` | Source inspected | **Partial / parity gap.** It is inspection-only; it does not automatically reconstruct and resume model-role execution. |
| `undo` | `forge undo <mission> [checkpoint] [--approve]` | E2E preview refusal + approved undo restores exact original content/hash | **Verified on Linux for tested text replacement.** Crash/partial rollback and Windows ACL/metadata preservation remain unverified. |
| `freeze` / `unfreeze` | `forge freeze status/now`, `forge unfreeze --approve` | Source unit tests and CLI handlers present; supplied Cargo test logs report freeze tests passing | **Implemented, reported tested.** Direct CLI fail-path and race testing should be added. |
| `sandbox status/self-test` | `forge sandbox status/self-test` | Direct test: status says NOT IMPLEMENTED; self-test returns nonzero and executes no command | **Intentionally disabled / release blocker.** TypeScript reference has a fixed Linux namespace probe; neither is qualified to run arbitrary user projects safely. See `docs/SANDBOX_DESIGN_AND_TEST_SPEC.md` for the platform-specific design and adversarial release gates; that document is a specification, not implementation evidence. |
| `memory propose/add` | Rust has `forge memory save`, not TS proposal preview/confirm flow | Source comparison | **Parity gap.** Add a review-first proposal command and explicit consent/decline semantics; currently Save is a different CLI contract. |
| `memory list/search/show/edit/forget/verify/reindex/export/import/reset` | Rust equivalents exist, mostly with Rust-specific arg names/semantics | Source comparison; memory tests reported in Cargo log; not all commands independently exercised | **Partial.** CLI argument/result parity is not proven. Test scope isolation, import all-or-nothing, index rebuild, destructive confirmation, audit and editor interaction. |
| `memory open` | No Rust equivalent | Source command comparison | **Missing.** Add a non-destructive command to show local vault path or formally retire this TS command after documenting migration. |
| `pilot` | `forge pilot` plus `forge decisions list/show` | Rust source includes offline/consented live brief and persisted history; supplied unit tests reported passing | **Implemented, not fully verified.** No independent live fixture E2E or parity test of decision record import/tamper exists. No cost-trigger logic is allowed. |
| Evidence listing/show | Rust adds `forge evidence list/show` | Rust unit tests and source exist; not in TS router as a user-facing top-level command | **Rust extension, not parity requirement.** Integrity verifies bytes/hash, not truth; workflow wiring is incomplete. |
| TypeScript `options.ts` | Not separately routed in TypeScript command router; no Rust CLI command | Router was inspected; the file is not imported by the active command router | **Not classified as a lost public command.** Re-evaluate only if another official spec proves it was meant to be user-facing. |

## Module/architecture parity

| TypeScript module family | Rust port / location | Status and action |
|---|---|---|
| `src/cli/command-router.ts`, output, command files | `src/main.rs`, Clap enums/handlers | Most routed commands are present, but memory proposal/open and workflow semantics differ. Maintain the table above as acceptance coverage. |
| `src/core/errors.ts`, `ids.ts`, `mission-state.ts`, `types.ts` | `src/error.rs`, `mission.rs` | Typed errors/state/contracts exist. Compare serialization/state fixtures and edge transitions. |
| `src/config/*`, `src/privacy/project-home.ts` | `src/config.rs`, `src/storage.rs` | Core strict parsing and local state exist. Windows private ACL setup/validation is incomplete. |
| `src/audit/jsonl.ts` | `src/audit.rs` | JSONL audit, locking, bounds/redaction exist; crash/partial-write and OS-specific behavior need more tests. |
| `src/context/repository.ts` | `src/context.rs`, `src/files.rs` | Bounded index/context exists; ignore semantics and scoring/ranking are documented source gaps. |
| `src/agents/workflow.ts` | `src/workflow.rs`, `src/provider.rs` | Sequential six-role proposal path exists and E2E fixture passed. DevOps execution is blocked; no independent agents or tool-running planner. |
| `src/providers/*` | `src/provider.rs` | OpenAI-compatible, Ollama-compatible, Anthropic and Google code paths exist; supplied unit tests report adapter coverage. No external live service result is claimed. |
| `src/safety/secret-redaction.ts`, `path-policy.ts`, `policy-engine.ts`, `freeze.ts` | `src/redaction.rs`, `src/files.rs`, `src/config.rs`, `src/freeze.rs`, workflow checks | Redaction/path/freeze controls exist but full policy-engine equivalence is not proven. Heuristic redaction is not a guarantee. |
| `src/sandbox/*` and OS drivers | Only fail-closed command surface in Rust; no `sandbox.rs` driver | **Major missing implementation.** Port reviewed Linux isolation, then macOS/Windows isolation separately. Do not add host shell fallback. |
| `src/tools/safe-files.ts`, `patch-apply.ts` | `src/files.rs`, `src/patches.rs` | Read-only tools and approval/checkpoint/undo work for tested scenarios. Mutation support is limited to existing text files and requires adversarial/crash testing. |
| `src/persistence/mission-store.ts`, `message-log.ts` | `src/mission_store.rs`, `src/message_store.rs` | Checksum/project-bound state and hash-chained transcript exist. Automatic resume recovery is not wired. |
| `src/memory/markdown-store.ts`, `types.ts` | `src/memory.rs` | Local notes, scope, audit, import/export and confirmations exist; proposal UX differs and should be ported. |
| `src/decisions/trigger.ts` | `src/decision_arena.rs` | Offline brief/live decision flow/history exist. Confirm exact ranking/evaluation semantics against TS tests. Estimated money/cost triggers are prohibited. |
| `src/evidence/store.ts`, `src/verification/completion-gate.ts` | `src/evidence.rs`, `src/mission.rs`, `src/workflow.rs` | Data model/store exists. **Completion gate not fully called from live mission workflow**; wire it to trusted evidence and fail closed. |

## Blocking work before general public release

1. **Implement and independently review OS sandboxes.** Linux, macOS and Windows require platform-specific designs and negative tests for filesystem, network, process, resource, escape and cleanup behavior. Until then, no build/test/shell/browser execution.
2. **Wire evidence and completion gate.** A mission must not report complete without actual, correctly bound, tamper-detectable test/build outputs from an approved sandbox.
3. **Close public CLI parity gaps.** Memory proposal preview, `memory open`, import/export/reset failure atomicity, resumed mission reconstruction, and any remaining TS semantics.
4. **Increase patch scope safely.** File create/delete/rename and unified-diff handling, multi-file atomicity/rollback, mode/ACL preservation, conflict tests and crash recovery.
5. **Finish context/index correctness.** Full intended ignore rules, stale-index checks, scoring/ranking and prompt-injection tests.
6. **Cross-platform qualification.** Actual macOS/Windows build/test logs; Windows private state ACL enforcement; handle/hardlink tests; Unicode/path/permission tests.
7. **Supply-chain release.** Dependency advisory scan, SBOM, full license inventory, clean install/upgrade/uninstall, versioned release artifacts, checksums and provenance.
8. **Reproduce exact build evidence.** The supplied Cargo logs report 70/70 tests, Clippy and release build passing. Explicit successful transcripts for `cargo fmt --all -- --check` and `cargo check --all-targets --locked` are missing and must be rerun/captured.

## Cost-policy invariant

No estimated/predicted USD values, guessed price tables, `max_cost_usd`/equivalent budgets, Decision Arena price triggers, or auto-stop triggered by estimated/provider-reported money. Actual provider-reported usage/cost may be shown after an approved request; absent/invalid cost remains `unknown`. Timeouts, cancellation, token caps and security resource limits remain valid separate safeguards.

## Release decision

**Not 100% complete; not production-ready for general-purpose untrusted command execution.** Linux CLI behavior has meaningful evidence, but the sandbox, evidence/completion wiring, documented parity gaps, cross-platform qualification and supply-chain review are release blockers.
