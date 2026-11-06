# Do Code — competitor adoption plan (researched 2026-11-07)

Sources: Claude Code feature docs (five extension points), OpenCode 2.x
shared-service architecture, Codex CLI 0.147 plugin/thread model, Hermes
multi-model learning, Antigravity parallel-agent desktop. Full links in the
session evidence.

## What the leaders converged on (the 2026 baseline)

| Mechanism | Claude Code | Codex | OpenCode | Do Code today |
|---|---|---|---|---|
| Hooks (deterministic lifecycle) | ✅ core | ✅ | ✅ | ❌ (risk engine only) |
| Skills (on-demand instruction folders) | ✅ core | ✅ | ✅ | partial (FORGE.md/AGENTS.md + playbooks) |
| Subagents (separate context windows) | ✅ | ✅ | ✅ | partial (sequential perspectives + adaptive dispatch) |
| Plugins (bundled distribution) | ✅ | ✅ 0.147 | ✅ + desktop | ❌ |
| MCP client | ✅ | ✅ | ✅ | ❌ (sandbox-gated) |
| OS sandbox + approval modes | ✅ | ✅ granular | ✅ | ✅ risk 0-5 + approval memory; driver primitives; matrix pending |
| Desktop/web over one engine | ✅ (IDE+web) | ✅ cloud | ✅ 2.x shared service | ❌ (CLI only; desktop planned) |

## Adoption order (each phase TDD, gate-verified, honest)

**Phase A — prove the sandbox (in flight).** 26-case matrix on the free
GitHub `windows-2025` runner; flip `is_qualified()` only on full pass.
Everything else in this plan depends on this gate.

**Phase B — hooks.** Deterministic user commands at lifecycle points
(pre-apply, post-apply, on-verify-fail, on-complete), stored in
`forge.config.toml` + `.docode/hooks/`, executed through the SAME risk
engine + sandbox (a hook is never above policy). This is the single
highest-value adoption: it makes Do Code automatable without model-driven
nondeterminism.

**Phase C — skills.** `AGENTS.md`/`FORGE.md` stay as project policy; add
`.docode/skills/<name>/SKILL.md` folders loaded on demand (invocation by
name or keyword match into the context pack — the context engine already
ranks bounded context, so skills are just a new priority class). Cross-tool
compatibility target: Claude/Codex skill folders readable as-is.

**Phase D — plugins.** Bundle = skills + hooks + manifest (capability
declared, consent-gated per capability — already the risk-engine model).
Distribution: git repo / folder install first; marketplace later.

**Phase E — MCP client.** Only after Phase A passes: MCP tool calls are
process spawns → they MUST run inside the same qualified sandbox. Gated,
never free-running.

**Phase F — desktop app (OpenCode 2.x pattern).** The OpenCode lesson is
"one engine, many faces": a shared background service with SDK/API, then
CLI/TUI/desktop/web as views. Do Code's engine is already library-shaped
(pure functions, typed stores, fail-closed gates) — Phase F adds a local
service exposing the mission/evidence/task APIs, then a desktop shell
(Tauri-class) as a second front end. No engine rewrite needed.

**Adopted-already (this round):** unified per-call retry with Retry-After
(Claude Code), rate-limit pause-not-destroy (OpenCode 2.1.2), per-role
model routing (Hermes/Codex), AGENTS.md priority context (Codex), risk
0-5 + approval memory (Claude Code permission modes, but remembered).

**Not adopting (with reasons):** background daemon as default (adds attack
surface + complexity before the sandbox is qualified); cloud task runners
(no hosted backend; local-first is a product principle); ads/sponsored
anything (contradicts the honesty contract).
