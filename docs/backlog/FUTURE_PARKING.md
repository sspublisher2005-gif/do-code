# Future parking lot — ideas not built, not forgotten

**Rule:** each parked idea has a reason and an **unpark condition**. If the condition is met, move it to `FEATURE_LEDGER.md` with a new status and append to `DECISION_LOG.md`. No idea is silently dropped.

| Idea | Why parked | Unpark condition |
|---|---|---|
| Virtual company / multi-department agents (CEO/QA/growth teams) | Product theater; no working execution loop yet; 100+ role prompts | V1.0 gate passes AND 3 real users ask for team workflows |
| Game engine from scratch / PUBG / CoD / Blender / ChatGPT | Impossible for any agent in one prompt; fantasy target | Never as a product claim. Games *on* Godot/Bevy are user tasks, not a feature. |
| AGI claims | Unverifiable and dishonest | Never |
| Real LSP / tree-sitter repo map | Aider owns it (graph-ranked, token budget); Serena MCP 10× slower than grep (Claude #45132) | Eval shows our brace outline (C-005) loses on renames across modules |
| Supervisor + worker context separation | Claude ships it free; Anthropic +90.2% on research but ~15× tokens and weak coding payoff | Context-precision eval shows a gain on our 30 tasks |
| 1000-micro-decision coherence | METR rejects framing; >4h tasks <10% success | A benchmark exists and we score on it |
| Cryptographic / SCITT action receipts | No demand signal; our SHA-256 manifests already bind evidence | 10 enterprise interviews require verifiable signed receipts |
| Self-pruning memory | Mem0 (market leader) is ADD-only; auto-delete risks user data | Eval proves pruning beats accumulate+retrieval AND user opt-in exists |
| Diff storytelling build (visual call-graph) | CodeRabbit "Explain" owns it; large build | Review-time eval shows our plain-text summary is insufficient |
| GitHub Spec Kit / spec framework | 150.9k★ owns it; no differentiation | Never build; integrate via config if users demand it |
| Plugin marketplace | No signing, version pinning, or rollback yet | Signed plugin format + 3 real plugins exist |
| TUI / desktop GUI | CLI not proven yet; IDE UX not Rust's strength | V1.0 passes AND users request a second surface |
| Enterprise compliance (SOC2/SSO/DPA) | 12–18 months, capital intensive | 10 enterprise interviews commit to purchase |
| Cloud long-running ticket→PR | Needs capital/cloud infra; Devin owns it | Local product has users and revenue |
| Linux sandbox driver | Windows not qualified yet | 26-case Windows matrix PASS + stable releases |
| macOS sandbox driver | Linux not done | Linux matrix PASS |
| Train our own model | No compute/data; violates "use best tool" principle | Never |
| "Full-only publish, no beta" | Proven wrong: every big agent iterated from weak v1 | Founder already reversed this (2026-11-21) |
| Bastion crate / Doc2Harness / IETF EQR as dependencies | Fabricated or unverified (fact-check 5262006) | Never, unless primary sources appear and Rule 1 passes |
| Hard cost stops / `--parallel` flag / forced browser tests | Explicit founder rules against them | Never |

## How to use this file
When a new idea arrives: (1) check this file; (2) if parked and condition unmet → do not build, record the request in DECISION_LOG; (3) if condition met → promote to FEATURE_LEDGER with status FUTURE or PLANNED; (4) if rejected forever → leave here with reason.
