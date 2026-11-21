# Why big companies haven't built the combined agent — and is the gap real?

**Date:** 2026-11-21 · **Rule 1 applies** (supreme rule in /docs/PDR.md).

## The three reasons, verified

### 1. Business model / vendor lock-in — VERIFIED, ERODING
- Anthropic Commercial Terms §D.4: no "competing product… to train competing models or resell"; OpenAI Business Terms §3.3(e): no Output "to develop AI models that compete with OpenAI" (official legal pages, fetched 2026-11-21).
- HN: "in Codex you can only run OpenAI models and not Gemini" (HN 47140322).
- **Erosion:** Claude Code enterprise docs support Bedrock, Vertex, Microsoft Foundry, third-party providers, LLM gateways, `allowedProviders`; Cursor's model table spans Anthropic/Google/OpenAI/DeepSeek/Moonshot/Meta. Lock-in now sits at the **model** layer, not the harness layer.

### 2. Demo-velocity trap — PARTIALLY VERIFIED
- 100-line mini-swe-agent scores near SOTA (65% vs ~70% for complex scaffolds, HN 44746077) — models absorb scaffolding, so elaborate harnesses may not market well.
- Benchmark saturation reports (self-reported, UNVERIFIED).
- Churn: Gemini CLI unpaid tier replaced by Antigravity after 11 months; not optimized for newest model (official docs).
- **Refutation:** labs do ship durable substrate — MCP, Agent SDKs, hooks, skills, CLAUDE.md/GEMINI.md, ACP, cloud scheduling. The trap is in model marketing, not uniformly in platform engineering.

### 3. Enterprise liability — VERIFIED as a brake, not impossibility
- Both terms: "AS IS", liability capped at 12 months fees; OpenAI Beta: "WILL HAVE NO LIABILITY" (§12.3). Customer must evaluate outputs.
- Trust incidents: researcher claim of prompt-steganography keyed to ANTHROPIC_BASE_URL/timezone in Claude Code 2.1.196 (thereallo.dev — researcher claim, NOT Anthropic-confirmed, HN 2445 points); issue #107757 (805👍) account email leaked in User-Agent, patched opt-out with follow-up gaps on 2.1.319/2.1.327.
- **Refutation:** labs sell compliance — Claude Code SSO/CMEK/managed settings/telemetry disable; Cursor SOC 2 Type II. Liability is priced and contracted around, not avoided.

## Is the gap real or wishful thinking?
**Real as a portability + trust + cost-routing gap.** Evidence: an entire tool ecosystem exists solely because labs don't provide portability — Beacon ("agent data should not be locked behind any frontier lab's ecosystem"), Weave Router, Emdash (21 agent CLIs), session-hub — HN 50182883.

**Wishful as a single "better model" play.** HN pushback: "Why should we invest long time into your 'ADE'… Won't that be addressed eventually by the CLIs themselves?" and "Custom AI tools have an uphill battle… went back to Claude Code" (HN 47140322). Model capability absorbs scaffolding over time, so feature moats decay.

## What this means for Do Code (Rule 1)
- Build the **neutral layer**: portable session/memory format, config compatibility across harnesses, model-agnostic routing, verifiable zero-telemetry + SBOM, OS-enforced sandbox with receipts. These decay slowest because they conflict with lab business models.
- Do **not** build: a me-too agent with a slightly better prompt; a competing model; an IDE.
- Risk: trust/zero-telemetry differentiation inferred from one issue + one HN thread — validate with 10 enterprise interviews before committing major engineering time.

## Limitations
Reddit 403 / websearch 402; no market sizing; no hands-on binary testing; prompt-steganography is a researcher claim; all vendor performance numbers UNVERIFIED.
