# Forge CLI — OS Sandbox Design and Adversarial Test Specification

**Date:** 2026-10-26  
**Product:** Forge CLI (`forge`)  
**Status:** Design/acceptance specification only — **not implemented and not a security certification**.  
**Scope:** Safe execution of compiler, test, lint, and explicitly approved project commands initiated by the AI coding workflow.

## 1. Current truth and release gate

The Rust release candidate has no native command-execution sandbox. `forge sandbox status` reports that the sandbox is not implemented, and `forge sandbox self-test` fails closed before running a command. This behavior is correct and must remain until a native driver has been implemented, independently reviewed, and passed the negative test suite in this document.

The TypeScript reference contains an experimental Linux namespace prototype and documents why it is **not production-ready**: no Landlock enforcement, no seccomp-BPF enforcement, incomplete aggregate resource control, `/proc` limitations, snapshot-copy race risk, and no independent penetration test. It must not be wired into arbitrary mission execution merely because it has tests.

**Hard rule:** no direct host-shell fallback, no automatic downgrade to a weaker driver, no execution when a required primitive is missing, and no successful verification claim without persisted evidence from commands actually run inside the approved sandbox. A `sandbox status` success means only that a capability probe ran; it does not independently certify isolation.

## 2. Threat model

Commands, scripts, dependencies, generated code, build hooks, tests, and model-produced instructions are untrusted—even when they originate in the user's repository. Assume an attacker can control project filenames/content, a package lifecycle script, a test, a compiler plugin, a generated binary, or a model response. The sandbox must protect the host user account and other applications from a malicious or compromised child process.

Protect:

- Home directory and personal files; SSH keys, cloud credentials, Git credentials, package-manager tokens, browser profiles, password stores, `.env` files, provider keys, other repositories, and Forge's private state/audit/memory.
- Host root/system files, device nodes, process namespaces, sibling processes, host IPC sockets, container/VM sockets, and inherited file descriptors.
- Host networking, loopback services, LAN endpoints, metadata endpoints, DNS, and unexpected outbound/inbound TCP/UDP.
- Host availability from CPU, RAM, PID, disk, output, file-descriptor, fork-bomb, and long-lived-process exhaustion.
- Correctness of evidence: stdout/stderr/status/timeouts/test reports must be bound to the command, sandbox ID, workspace snapshot/hash, and driver capabilities actually used.

Out of scope for this local sandbox alone: kernel zero-days, physical access, malicious firmware, compromised trusted host kernel/hypervisor, and every covert channel. State the residual risk. Do not use `safe`/`secure` marketing wording that exceeds the measured threat model.

## 3. Execution architecture

Use a narrow driver interface behind a policy-enforcing resolver. Suggested Rust abstraction:

```text
SandboxRequest
  workspace_snapshot_id
  argv: Vec<OsString>       # executable plus arguments, never interpolated shell text
  cwd: path inside snapshot
  environment: allowlisted key/value pairs
  network_policy: DenyAll | ExplicitAllowlist(...)
  resource_limits: wall_time, cpu, memory, pids, file_size, disk, output
  approval: explicit user approval record for the task/command class

SandboxHandle
  identity: unique sandbox/run ID
  driver + OS/kernel/security primitive versions
  workspace input hash and writable output root
  wait/cancel/kill_and_reap/collect_evidence operations
```

Requirements:

1. Separate the policy decision from driver implementation. Every execution request is checked at the moment it runs; earlier model approval is not a permanent authorization.
2. Pass argument vectors directly to process creation. Do not construct `sh -c`, PowerShell, `cmd /c`, or other command strings from task text. Shell use must be an explicit, policy-governed user request inside the sandbox and not a fallback.
3. Take a consistent disposable snapshot or immutable source mount, with a writable output/work tree, and keep secrets and the host's `.forge` directory outside it. Never execute against the live host working tree if the sandbox contract promises isolation.
4. Build an intentionally minimal environment. Drop inherited environment variables except a small allowlist; never forward provider API keys, tokens, home/profile variables, agent sockets, SSH/GPG agents, proxy credentials, `RUSTC_WRAPPER`, arbitrary `PATH` overrides, or inherited descriptors by default. Pin a known executable search path and record the resolved executable identity.
5. Network is **deny-all by default**. If a user explicitly needs downloads, grant a narrowly scoped temporary allowlist in a distinct policy mode. Network allowance must be surfaced in confirmation UI, applied by the OS driver, recorded, and tested. Do not assume DNS-only denial means network isolation.
6. Resource limits and process-tree management are mandatory, not optional optimizations. On timeout/cancel/output-limit, terminate the entire sandbox process tree, reap it, verify it is gone, then collect bounded output.
7. If the policy engine, snapshot, driver setup, network controls, ACLs, resource limits, process containment, output bounds, or cleanup fails, return a typed safety error and execute **zero** project commands.
8. Persist an evidence record for every actual command: exact argv safely redacted, working directory in sandbox-relative form, environment variable names but not secret values, snapshot hash, executable hash/version where practical, driver ID and kernel/OS, enforced capability list, start/end timestamps, exit code or terminating signal, timeout/cancel cause, stdout/stderr hashes and bounded content, and test report hashes. Hashes prove integrity of captured bytes, not the truth of the test's meaning.

## 4. Platform driver design and limitations

### 4.1 Linux (including WSL2 only after separate tests)

Preferred approach: a well-established, maintained unprivileged namespace runner such as Bubblewrap (`bwrap`) with user/mount/PID/network/IPC/UTS namespaces, carefully constructed mounts, plus Landlock and seccomp-BPF where required and supported. Use a dedicated implementation/adapter with argument-vector invocation. Any external helper is a dependency that must be detected, versioned, and tested—not assumed to exist.

- Construct a minimal filesystem view. Mount the toolchain/system runtime read-only; mount a disposable workspace snapshot read-write; create a private ephemeral `/tmp`; omit host home, credentials, other repositories, device files, IPC sockets, container sockets and the original `.forge` state. Add only runtime directories required by explicitly tested tools. Handle `/proc` only when it is mounted safely and policy permits it.
- Create user, mount, PID, network, IPC and UTS namespaces as applicable. Drop capabilities and set `no_new_privs`; use a seccomp-BPF profile denying unnecessary dangerous syscall classes. Do not claim seccomp is active unless the runtime can inspect/prove the installed filter (a successful `prctl`/install call is one item in a testable capability report, not the whole guarantee).
- Apply Landlock filesystem rules to the minimum read-only runtime and workspace paths plus writable scratch directory. Use a runtime ABI probe and explicitly construct the handled-rights mask for the detected ABI. Apply TCP/UDP rights only where supported; do not assume old kernels enforce network rights. If the policy requires a primitive the kernel/runtime does not support, fail closed unless a separately reviewed alternative provides the same required guarantees.
- Enforce timeout and process tree cancellation; use cgroup v2 aggregate limits if reliably delegated/available, and per-process limits as defense in depth. A per-process `RLIMIT_AS`/`prlimit` alone is not an aggregate process-tree memory cap. If the contract requires aggregate memory and it cannot be set, do not mark the capability as enforced.
- Avoid source snapshot TOCTOU: use directory descriptors/`openat2`-style constrained traversal where supported or a race-resistant snapshot method; reject/revalidate symlinks, hard links, mount crossings and file type changes. Hash the actual execution snapshot, not merely the original tree before copying.
- Disallow networking in the isolated network namespace by default. Prove host loopback, private/LAN IPv4/IPv6, DNS, UDP, Unix domain sockets, and metadata destinations are unreachable. Landlock alone is not a complete network namespace.

Official documentation: [Linux Landlock userspace API](https://docs.kernel.org/userspace-api/landlock.html), [seccomp filter](https://docs.kernel.org/userspace-api/seccomp_filter.html), [namespaces overview](https://man7.org/linux/man-pages/man7/namespaces.7.html), [Bubblewrap project](https://github.com/containers/bubblewrap). Landlock is an additional layer; its ABI-dependent rights and documented limitations must be respected. Do not describe Landlock alone as a complete sandbox.

### 4.2 macOS

Do not improvise an undocumented Seatbelt/sandbox-exec profile and do not treat a plain child process as isolated. Choose a supported, maintainable, reviewed boundary: for example, a properly signed and entitled App Sandbox helper whose file/network entitlements and child-process behavior have been demonstrated with a test harness, or a well-isolated disposable VM/runtime with strictly bounded workspace mounts, networking, and resources. Record which mechanism is actually used and its version/configuration.

- The workspace is the only writable persistent mount; system/toolchain inputs are read-only; secrets, home directory, SSH/cloud credentials, unrelated repositories and Forge state are inaccessible.
- Network is blocked by default at the isolation boundary, not only by an application convention. Allowlisting requires independent packet-level tests.
- Child process inheritance, helper entitlements, filesystem bookmarks/permissions, temporary-directory behavior and process-tree cleanup must be proven with signed/release builds on each supported macOS version/architecture.
- Verify codesigning, entitlements, runtime version, permissions and whether the sandbox is active. If OS policy prompts or user approvals are needed, explain them and avoid silently granting broad access.

Official documentation: [Apple App Sandbox](https://developer.apple.com/documentation/security/app-sandbox), [Configuring the macOS App Sandbox](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox). Documentation is not evidence that the Forge helper has correct entitlements; actual signed binary tests are required.

### 4.3 Windows

Use layered isolation, not NTFS ACLs or a Job Object alone. A reviewed driver should combine an AppContainer (or an equivalently reviewed stronger boundary), restricted access token/low privilege, explicit filesystem/registry/network capabilities, a private workspace directory with a correctly protected DACL, and a Job Object configured with CPU/time/memory/process controls and kill-on-close. Create/assign the child to the job before untrusted code starts (e.g. create suspended then assign and resume) and disallow breakaway behavior. If assignment or any required mitigation fails, terminate before resuming/executing.

- Create `.forge` and sandbox roots with explicit DACLs; read back and verify owner/ACLs for the actual account and inherited ACEs. Do not rely on POSIX mode equivalents.
- AppContainer file/network permissions are deny-by-default except for deliberately granted capabilities; do not give the workspace or network more rights than required.
- Remove dangerous privileges and add a restricted SID using the Windows restricted token APIs; verify token properties before launch. Job Objects provide group management/resource limits, but do not by themselves isolate filesystem or network.
- Disable debug/trace/process interaction where feasible; no broad inherited handles, no user profile, no host secrets. Ensure descendants stay in the job and termination of the Forge handle kills/reaps them.
- Test Windows Defender/SmartScreen/AV interference and long/Unicode paths as behavior/reliability concerns, but do not weaken security settings to get tests green.

Official documentation: [AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation), [restricted tokens](https://learn.microsoft.com/en-us/windows/win32/secauthz/restricted-tokens), [CreateRestrictedToken](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken), [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects). A Job Object controls associated process groups/resources; it is not a substitute for file/network isolation.

## 5. Capability contract

Report individual facts, not a single vague “secure” boolean. The `sandbox status`/evidence interface should include at least:

- platform/architecture/kernel or OS build;
- driver name/version and effective runtime identity;
- filesystem isolation, secret exclusion, workspace-write boundary, host-path denial and symlink/hard-link/mount-crossing protections;
- network deny-all status and tests (DNS, TCP, UDP, IPv4/IPv6, loopback/LAN);
- process-tree containment and no-breakaway configuration;
- CPU, memory, PID, disk/file-size, wall-clock and output limits actually applied;
- clean environment/FD handling;
- timeout/cancel/kill/reap success;
- last successful self-test timestamp and test report hash.

Do not allow a caller/model to mark a capability `true`; it must be supplied by the driver after setup and verified by an external test from within/outside the sandbox. Do not treat a config setting as proof of enforcement.

## 6. Required adversarial acceptance tests

Each negative test must assert BOTH: the malicious operation was denied, AND no forbidden state changed. Run in disposable VMs/runners. Attach OS version, kernel/runtime version, driver version, exact command, exit code, evidence/log hashes, and cleanup result. No test may be silently skipped and still pass release gating.

| ID | Attack / probe | Required result |
|---|---|---|
| FS-01 | Read `$HOME`, `.ssh`, cloud CLI credentials, `.git-credentials`, package tokens, browser profile, provider key, another repository | Permission denied or file absent; no secret bytes appear in stdout/stderr/audit/evidence |
| FS-02 | Write `/tmp`, `/etc`, home, parent of workspace, sibling repo, `.forge`, source host tree | Only explicit sandbox scratch and sandbox workspace are writable; original host file hashes unchanged |
| FS-03 | `../` traversal, absolute paths, Windows drive/UNC/device paths, alternate data streams | Denied; no outside file created or modified |
| FS-04 | Symlink and hard-link to secret or outside file; replace path while copy/open; rename/mount race | Escape denied; verify using descriptor/handle and post-operation hashes; race test repeated under concurrent mutation |
| FS-05 | Device nodes, procfs descriptors, host Unix socket/Windows named pipe, Docker/Podman/container socket | Absent/unreachable unless an exact resource is documented and approved |
| NET-01 | DNS queries and outbound TCP/UDP over IPv4/IPv6 | Denied in default profile and captured by outside observer |
| NET-02 | Connect to host 127.0.0.1/::1, gateway, LAN, cloud instance metadata, inbound listener, proxy | Denied; host fixture listener records zero accepted connections |
| PROC-01 | Fork bomb / huge process tree / double-fork / `setsid` / detached children / job breakaway | PID limit applies; all descendants killed and reaped after timeout/cancel; no orphan remains |
| PROC-02 | Read `/proc`, enumerate host PIDs, ptrace sibling, signal host process, debug/inspect parent | Denied or isolated to sandbox processes; host fixture process remains unchanged |
| RES-01 | CPU loop, memory pressure, large files, infinite stdout/stderr, many files/open FDs | Correct hard bounds; Forge survives, records termination cause, and cleans up; no host exhaustion |
| ENV-01 | Echo every environment variable; inspect inherited file descriptors/handles | Only allowlisted non-secret variables and intended stdio exist; no provider keys, agent sockets, credential handles or parent workspace descriptors |
| CMD-01 | Argument injection through spaces, quotes, shell metacharacters, Unicode/newlines, malicious filenames | Arguments reach the selected executable unchanged; no parent/host shell interpolation |
| EXEC-01 | Missing driver, missing privilege, old kernel/ABI, unavailable network restriction, bad ACL, setup race, cleanup error | Command executes zero times; state is BLOCKED/UNAVAILABLE, not “best effort” |
| LOG-01 | Secret in stdout/stderr/exception, ANSI/control escape, giant output or invalid encoding | Outputs bounded/escaped/redacted; raw secret is not written to terminal/audit; hashes and truncation are reported |
| PROOF-01 | Tests never run, runner exits nonzero, timeout, corrupt report, stale workspace hash, forged test-result JSON | `forge verify` refuses completion; no successful test evidence is fabricated or accepted without the executed command/snapshot/hash binding |
| LIFE-01 | Kill Forge during setup/run; disk full; crash between output snapshot and evidence commit; concurrent tasks target same workspace | No unsandboxed continuation, duplicate action, untracked mutation or false completion; recover state and either roll back or surface an explicit unresolved recovery state |

Also run property/fuzz tests over argv/config/path handling, patch parsing, workspace snapshot enumeration, and evidence serialization. Ensure high-severity test assertions fail the CI job if omitted.

## 7. Resource and policy baseline

Start with conservative documented defaults, configurable only within hard caps enforced by the driver: command wall-time; CPU time/rate; per-process and aggregate memory (do not confuse these); maximum PIDs; file-size and total writable disk/quota; open files; stdout/stderr combined bytes; filesystem entries; environment size; and log retention. Exact numbers should be set by benchmarked defaults and supported OS/runtime limits, not invented in source comments. Record each effective limit in command evidence. Missing or unenforced required limits block `productionReady`.

Default networking: deny all. Dependency installation/networking is a separate explicit consent mode with temporary allowlist and its own audit evidence. Do not pass the provider API key to project commands; AI provider calls happen in the trusted Forge process and the model should receive only consented bounded context.

## 8. Wiring into Forge CLI workflow

1. `forge sandbox status`: detect capabilities with small non-destructive probes and accurately distinguish `available`, `unavailable`, `not implemented`, and `test failed`.
2. `forge sandbox self-test`: run a deterministic, disposable adversarial smoke suite—not arbitrary user text—and persist the evidence report. It must not report PASS when a security primitive was unsupported or a test skipped.
3. `forge run`: may propose a patch without command execution, but cannot tell the user the code was tested. It must invoke the sandbox driver for allowed checks only after policy/approval.
4. `forge verify`: derives status from verified evidence bound to current workspace/patch hash, command, actual exit code, timeout state, and test report. A changed workspace invalidates stale results.
5. `forge resume`: reconstructs the sandbox/mission state safely; never blindly restarts an irreversible command. Require policy re-evaluation and user approval for commands requiring it.
6. `forge undo`: only manipulates explicit Forge checkpoints in the workspace; never trusts a sandbox to write arbitrary rollback paths. Check for post-apply user changes before mutation.
7. `forge audit`: record policy decision, driver/capability proof, approval, sandbox ID, command evidence reference, exit result and cleanup. Redact secrets.

## 9. Implementation sequence and release gates

- **S0 — Contract tests:** add all tests as executable test IDs and prove they fail against the no-driver baseline in the expected fail-closed manner. Test harness is part of the product.
- **S1 — Linux driver:** implement an adapter around a reviewed helper/mechanism; feature-probe namespaces, mounts, Landlock ABI, seccomp, network deny, limits, cleanup. Do not use the TypeScript namespace prototype as a production driver. Run all Linux adversarial tests on actual supported kernels, including a minimal/old unsupported kernel to prove fail-closed behavior.
- **S2 — Evidence wiring:** persist verified command output/test reports; connect evidence to the mission completion state and workspace/patch hashes; test falsification/stale evidence.
- **S3 — Windows driver:** implement AppContainer/restricted token/ACL/Job Object layers; unit tests for API failure and real adversarial runs on Windows CI; get security review.
- **S4 — macOS driver:** implement signed and correctly entitled isolated helper or vetted VM/runtime; verify the released binary's effective restriction; adversarial macOS tests on actual supported versions.
- **S5 — Review and release:** independent code review, dependency advisory/SBOM review, fuzzing, fail-closed matrix, test logs, reproducible release artifacts and documented residual risk.

A public general-use release is blocked until at least one supported driver completes adversarial tests and its completion evidence is wired correctly. For any OS without a fully proven driver, command execution must remain disabled and the support table must say so. A Linux-only release can be considered only if all other OSes are explicitly not supported for execution and this limit is prominent in docs/product claims.

## 10. Official references

- Linux kernel Landlock API and ABI rights/limitations: https://docs.kernel.org/userspace-api/landlock.html
- Linux kernel seccomp filter: https://docs.kernel.org/userspace-api/seccomp_filter.html
- Linux namespaces man page: https://man7.org/linux/man-pages/man7/namespaces.7.html
- Bubblewrap project and issue history: https://github.com/containers/bubblewrap
- Apple App Sandbox: https://developer.apple.com/documentation/security/app-sandbox
- Apple App Sandbox configuration: https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox
- Windows AppContainer isolation: https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation
- Windows restricted tokens: https://learn.microsoft.com/en-us/windows/win32/secauthz/restricted-tokens
- Windows `CreateRestrictedToken`: https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-createrestrictedtoken
- Windows Job Objects (process groups/resource limits and breakaway considerations): https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects

These sources explain available primitives, not whether Forge CLI integrated them correctly. Only actual code inspection, runtime tests, independent review, and evidence from supported OS runners can make that determination.
