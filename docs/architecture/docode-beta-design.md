# DoCode Beta — Technical and TUI Design Plan

**As-of date:** 2026-11-24
**Status:** Planning document. No implementation started.

## 1. Current Architecture Assessment

### What works (keep as-is)
- **Rust + Tokio + Clap + Serde + reqwest** — solid foundation, no changes needed
- **Evidence ledger** (evidence.rs, audit.rs, message_store.rs) — hash-chained, tamper-evident. Keep.
- **Risk engine** (risk.rs) — 0-5 scoring with approval memory. Keep.
- **Windows sandbox** (sandbox.rs) — AppContainer + Job Object. Keep, extend to Linux/macOS later.
- **Patches** (patches.rs) — edit/create/delete/rename with checkpoints. Keep, add unified diff support.
- **Per-role routing** (router.rs) — now has cost tiers. Keep.
- **CI pipeline** (rust-ci.yml) — 6 jobs, 3 OS x 2 toolchains. Keep.
- **Outline** (outline.rs) — compact symbol extraction. Keep.

### What must change (incremental, not rewrite)
- **provider.rs** — add tool-call schemas, streaming (SSE), tool result parsing. Currently single-shot text.
- **workflow.rs** — replace fixed 6-role linear pipeline with tool-call loop. Keep verify()/complete() gate.
- **agent_loop.rs** — wire tool loop as alternative to fixed pipeline. Already has repair loop + false-success gate.
- **context.rs** — replace substring ranking with repo traversal + on-demand file reading. Remove 96KB cliff.
- **patches.rs** — add unified diff / search-replace apply. Keep checkpoint/undo.
- **tools.rs** — add per-tool permission policy (allow/ask/deny). Currently Deny/Execute only.

### What to add (new modules)
- **tools/registry.rs** (or extend tools.rs) — tool definitions with JSON schemas
- **tools/executor.rs** (or extend tools.rs) — tool dispatch with permission check
- **context/repo_map.rs** (or extend context.rs) — file tree + symbol index

### What NOT to change
- Evidence/audit architecture (keep as-is, it's the moat)
- Sandbox driver (keep, extend later)
- Risk engine (keep, wire into tool dispatch)
- CI pipeline (keep, add eval job later)

## 2. Tool-Call Loop Architecture

### The loop (pseudocode)
```
messages = [system_prompt, AGENTS.md, task]
while budget_remaining:
    response = provider.generate(messages, tools, stream=true)
    record evidence + hash-chain message_store
    if response.tool_calls is empty:
        return final answer
    for call in response.tool_calls:
        risk_score = risk::score_command(call)
        if risk_score >= 3: ask approval (approval memory)
        result = tools::execute(call)  # sandboxed if command
        evidence::put(result, kind="tool_result")
        messages.push(tool_result)
    compact if over budget (summarize old turns, keep prefix stable)
```

### Tool definitions (start with 4)
1. **read_file(path, offset, limit)** — bounded read, redacted, max 64KB
2. **edit_file(path, old, new)** — search/replace, checkpoint before
3. **run_command(command, timeout)** — sandboxed, captured output, receipt
4. **search(pattern, path)** — ripgrep, bounded results

### Provider changes
- OpenAI: add `tools` array to request body, parse `tool_calls` from response
- Anthropic: add `tools` array, parse `tool_use` blocks
- Google: add `functionDeclarations`, parse `functionCall` parts
- Ollama: OpenAI-compatible format

### Stop conditions
- No tool_calls in response → final answer
- Budget exhausted (tokens or turns)
- Consecutive failures > 2 (repair limit)
- User cancels (Ctrl+C → graceful stop, preserve state)

## 3. Permission Model

### Per-tool policy (extend tools.rs)
- **allow**: tool runs without approval (read_file, search, outline)
- **ask**: tool requires approval (edit_file, run_command)
- **deny**: tool never runs (none by default)

### Risk-based escalation
- risk.rs scores each command 0-5
- Score >= 3 → ask (even if tool is "allow")
- Score >= 5 → deny (even if tool is "ask")
- Approval memory: "ask once, remember" per (tool, args, cwd) tuple

### Approval UX
- Show: tool name, arguments, risk score, reason
- Options: Allow once / Allow always (this tool+args) / Deny
- Timeout: 60s default, then deny

## 4. TUI Design (Phase F)

### Principles
- Terminal-first, not web-dashboard-in-terminal
- Streaming output (tokens appear as generated)
- Clear distinction: reasoning (dim), tool activity (colored), results (normal)
- Per-hunk diff review (not full-file dump)
- Keyboard-driven approve/reject
- Minimal chrome, maximum content

### Screen states
1. **Input** — multiline prompt, history, slash commands
2. **Streaming** — tokens appearing, tool calls inline, progress indicator
3. **Permission** — modal overlay showing tool + args + risk + options
4. **Diff review** — per-hunk accept/reject, file list sidebar
5. **Session** — session list, switch, resume
6. **Error** — clear error, suggested fix, retry option

### Key bindings (proposed)
- Enter: submit prompt
- Ctrl+C: cancel current operation
- Tab: switch between input and output panes
- y/n: approve/reject (in permission mode)
- j/k: navigate hunks (in diff review)
- Ctrl+L: clear screen
- /help: show commands
- /quit: exit

### What NOT to do
- No animations or spinners (just text)
- No excessive boxes or borders
- No huge activity logs (collapsible)
- No redundant status bars
- No web-style layouts

## 5. Failure and Recovery

### Tool execution failures
- Command timeout → kill process tree via Job Object, report timeout
- Command OOM → job kills, report OOM
- Fork bomb → active process limit (64) kills
- Network failure → retry with backoff (3 attempts), then report
- Invalid tool args → reject before execution, report validation error
- File not found → report, suggest alternatives

### Model failures
- Provider 429 → pause mission, preserve state, retry after backoff
- Provider 5xx → retry (3 attempts), then fail with error
- Invalid JSON → retry (2 attempts), then report parse error
- Context overflow → compact (summarize old turns), retry
- Rate limit → pause, report, suggest cheaper model

### Cancellation
- Ctrl+C → graceful stop, preserve state, report what was done
- Mission state saved to disk (can resume later)
- No partial file writes (checkpoint before edit, restore on cancel)

### Recovery
- Resume from last checkpoint (mission state + context)
- Re-run failed step with same or different model
- Undo last patch (patches::undo)

## 6. Security Model

### What runs automatically (no approval)
- read_file (bounded, redacted)
- search (bounded, redacted)
- outline (read-only)
- list_files (read-only)

### What requires approval
- edit_file (show diff first)
- run_command (show command + risk score)
- Any tool with risk score >= 3

### What never runs
- Commands with risk score >= 5
- Commands that modify files outside workspace
- Commands that access credentials/secrets
- Network access (sandbox denies by default)

### Sandbox enforcement
- Windows: AppContainer (zero capabilities) + Job Object + DACL + sanitized env
- Linux: bubblewrap + Landlock + seccomp (planned)
- macOS: Seatbelt (planned)
- All commands run in sandbox; no unsandboxed execution

### Secret handling
- Redact before model sees output (redaction.rs)
- Redact before writing to logs/audit
- Never write API keys to disk
- Sanitized env (no credential-shaped vars passed to child)

## 7. Context Management

### Current (primitive)
- Substring ranking (path +2, symbol +2, entrypoint +1, AGENTS.md +10000)
- 96KB cap, max 20 files
- Truncation → refuse (cliff)

### Proposed (incremental)
- Repo traversal (respect .gitignore, skip binary/large files)
- On-demand file reading (agent reads what it needs)
- Tree-sitter repo map (symbols + ranges, token-budgeted)
- Prompt caching (keep prefix stable)
- Compaction (summarize old turns when over 80% capacity)
- Embeddings (optional, for large repos)

### Token budget
- System prompt: < 1,000 tokens (keep it small)
- Tool definitions: < 500 tokens (defer loading like jcode)
- Context pack: < 30% of context window
- Reserve 40% for output + tool results
- Monitor cache hits (jcode pattern)

## 8. Cross-Platform Support

### Phase 1 (Windows only)
- Windows 10/11, x86_64
- AppContainer + Job Object sandbox
- Tested on windows-2022, windows-2025 (CI)

### Phase 2 (Linux)
- bubblewrap + Landlock + seccomp
- Tested on ubuntu-latest (CI already runs fmt/check/clippy/test)

### Phase 3 (macOS)
- Seatbelt (sandbox-exec)
- Tested on macos-latest (CI already runs fmt/check/clippy/test)

### Principle
- One at a time, never parallel OS drivers
- Windows first → Linux → macOS
- Each platform needs its own 26-case adversarial matrix

## 9. Model Strategy

### Phase 1 (single provider)
- OpenAI-compatible (covers OpenAI, Ollama, and most providers)
- Hardcode as default, allow override via config

### Phase 2 (multi-provider)
- Add Anthropic (tool_use), Google (functionDeclarations)
- Per-role routing (already exists)
- Cost tiers (already exists)
- Fallback chain (already exists)

### Phase 3 (OAuth + local)
- Claude subscription login (OAuth)
- OpenAI subscription login
- Google subscription login
- Account router with quota failover (jcode pattern)
- Local model support (Ollama, in-process)

### Cost control
- Per-task budget cap (user-configurable)
- Provider-reported usage tracking
- Pre-flight cost estimate (rough, not guessed USD)
- Stop when budget exceeded (not mid-task — at safe boundary)

## 10. Testing Strategy

### Unit tests (existing)
- 141 passing (sandbox, qualification, outline, router, tools, etc.)
- Add: tool-call parsing, loop termination, permission policy

### Integration tests (existing)
- 10 CLI smoke tests (tests/cli_smoke.rs)
- Add: end-to-end doc run on a real repo

### Eval suite (new)
- 20 tasks: bug fix, feature add, test write, refactor, debug
- Each task: clear expected outcome, verification procedure, result record
- Deterministic automated checks where possible
- Human review where automated checks insufficient
- Run nightly, track pass rate over time
- Publish results (honesty: include failures)

### Benchmark (new)
- Terminal-Bench adapter (harbor framework)
- Aider polyglot (225 Exercism tasks, 6 languages)
- Own "Verification Bench" (tasks requiring auditable evidence)
- Publish scores + transcripts + failures

## 11. Unresolved Architectural Decisions

1. **Tool loop vs fixed pipeline:** Replace workflow.rs 6-role chain with tool loop? Or keep both (tool loop for agent mode, fixed pipeline for structured mode)? Recommendation: keep both, tool loop as default, fixed pipeline as --structured flag.

2. **Context management:** On-demand file reading vs pre-packed context? Recommendation: on-demand (agent reads what it needs), keep outline as a navigation aid.

3. **Permission UX:** Modal overlay vs inline prompt? Recommendation: inline prompt (less disruption, faster).

4. **Session persistence:** SQLite vs JSON files? Recommendation: JSON files (already using, simpler, auditable).

5. **TUI framework:** ratatui vs crossterm-only? Recommendation: ratatui (standard, well-documented).

6. **Streaming protocol:** SSE (Server-Sent Events) for all providers. Recommendation: yes, SSE is standard.

## 12. What to NOT Build (and why)

- Multi-agent company simulation — product theater, no working execution loop
- Game engine / PUBG / ChatGPT — impossible for any agent
- AGI claims — unverifiable
- Real LSP / tree-sitter repo map — Aider owns it; our brace outline is sufficient for v1
- Supervisor + worker context separation — Claude ships it free; weak coding payoff
- Cryptographic/SCITT action receipts — our SHA-256 manifests already bind evidence
- Self-pruning memory — Mem0 is ADD-only; auto-delete risks user data
- Diff storytelling build — CodeRabbit owns it
- Plugin marketplace — no signing/versioning yet
- Desktop GUI — CLI not proven yet
- Enterprise compliance (SOC2/SSO) — 12-18 months, capital intensive
- Train own model — no compute/data
- Cost-based stopping — explicit founder rule against it
- --parallel flag — explicit founder rule against it
- Forced browser testing — explicit founder rule against it