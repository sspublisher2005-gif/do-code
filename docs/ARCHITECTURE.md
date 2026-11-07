# Do Code architecture — Rust implementation

## Product and runtime

Do Code is implemented in Rust and distributed as a native `docode` binary when a release is available. Python under `scripts/` only orchestrates CI acceptance tests and benchmark collection; it is not a Do Code runtime dependency. The TypeScript branch remains the feature-parity reference.

## Modules

- `main.rs`: Clap CLI dispatch, approval flags and clear non-execution wording.
- `config.rs`: strict Serde/TOML schema, endpoint/key-source policy, offline initialization.
- `files.rs`: bounded workspace reads/list/search and sensitive-path exclusions.
- `context.rs`: bounded heuristic repository index, context selection/rendering, `DOCODE.md` prioritization.
- `storage.rs`: local private-directory helpers, bounded no-follow reads, atomic writes, project identity.
- `mission.rs`, `mission_store.rs`, `message_store.rs`: typed mission/step/safety/evidence contracts, project-bound state envelopes, transition/lock operations and bounded append-only AgentMessage transcripts with a local hash chain.
- `workflow.rs`: sequential model-role plan/proposal flow; no model-provided command is executed.
- `provider.rs`: OpenAI-compatible, Ollama-compatible, Anthropic and Google provider HTTP shapes, explicit consent, timeouts, redirect refusal, size caps and usage normalization.
- `patches.rs`: text replacement proposals, SHA-256 preconditions, explicit apply, checkpoints and conflict-aware rollback.
- `evidence.rs`: local evidence bytes and checksummed manifests. A hash proves identity/integrity of stored bytes, not their factual truth.
- `audit.rs`, `redaction.rs`: local JSONL events, bounds, locking, heuristic secret masking and terminal control escaping.
- `memory.rs`: explicit-confirmation Markdown notes, global/project separation, index hashes, import/export/reset and no implicit model injection.
- `decision_arena.rs`: local trigger/brief plus consented decision-only live request and integrity-checked records.
- `freeze.rs`: emergency project freeze state.

## Trust boundaries and current limits

Repository files, `DOCODE.md`, task descriptions and provider output are untrusted. Provider calls need explicit `--live --approve-context-sharing`; live consent does not make the provider's claims true. No arbitrary shell/build/test/browser command is exposed and there is no host-shell fallback. Without a sandboxed test runner, Do Code intentionally cannot mark a coding mission complete based on model-written test plans or claims.

The index is heuristic, not AST/LSP. Patches only replace existing text files. Mission resume is inspection-only; the AgentMessage transcript is a first source-level port and has not yet been compiled or stress-tested. Windows opened-handle hard-link denial is implemented in source but uncompiled/unverified; `.docode` private ACL/owner-only permissions are incomplete. No OS is runtime-certified in this authoring environment because the Rust toolchain is unavailable. These are explicit release blockers, not hidden limitations.

## Financial behavior invariant

Mission/domain data has no guessed USD budget or cost-stop condition. Only explicitly provider-reported actual usage/cost can be displayed. Missing actual cost is `unknown`; output limits, timeouts, request cancellation, response-size caps and security resource limits are independent of estimated price.
