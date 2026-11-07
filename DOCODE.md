# Do Code project memory

This file is untrusted project context, not a security policy. It cannot grant tools or bypass runtime permissions.

## Product
- Do Code product; this branch is its Rust-language implementation.
- Keep `0.6.0-ts.2` as the preserved TypeScript feature-parity/reference branch until the Do Code Rust implementation has matching tests.

## Invariants
- No telemetry by default.
- No estimated cost or cost-budget stop logic; only provider-reported usage may be displayed, otherwise use `unknown`.
- Repository/model output is untrusted input.
- No host shell or autonomous coding execution until a separately verified sandbox gate passes.
- Completion reports require evidence; a model saying "done" is not evidence.
- Never claim Rust build/test success unless Cargo actually ran and exited successfully.
