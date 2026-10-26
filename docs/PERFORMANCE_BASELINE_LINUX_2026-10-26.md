# Forge CLI Linux performance baseline — 26 October 2026

**Scope:** one Linux x86-64/glibc container, one supplied release ELF. These are measurements of this exact environment, not a promise or comparison against other coding agents. No network or model calls were made.

## Artifact identity

- Version: `forge 0.7.0-rs.1`
- Binary bytes: `8,052,120`
- SHA-256: `a861235a713b8375b6eeccdfc699ccda046e603e5e6eed4ef658cc6a8d704962`
- Environment: Linux 6.18.55, x86-64, glibc 2.41, Python 3.13.5 harness

## Warm-cache repeated-process latency

Collected by `scripts/verify_cli.py`; the measurement starts a new Forge subprocess each sample after warm-up. There is no clean-cache/cold-boot claim.

| Operation | Samples | Median | p95 | Maximum |
|---|---:|---:|---:|---:|
| `forge --version` new-process startup | 30 | 1.288 ms | 1.9 ms | 1.942 ms |
| `forge doctor` | 10 | 3.614 ms | 4.267 ms | 4.267 ms |
| `forge files list` | 10 | 3.786 ms | 5.765 ms | 5.765 ms |
| `forge config show` | 10 | 4.015 ms | 5.494 ms | 5.494 ms |

The preserved earlier 30-run report measured `--version` median 1.288 ms / p95 1.900 ms / max 1.942 ms, and command medians of 3.614 ms (`doctor`), 3.786 ms (`files list`), and 4.015 ms (`config show`). See `04_INDEPENDENT_VERIFICATION/independent_verify_cli_rerun_2026-10-26.json`; the latest 10-run report is `BLACK_BOX_BENCHMARK_2026-10-26.json`. Different run count/sample noise means those reports should not be combined into one statistical claim.

## Peak resident memory (RSS)

Collected by `scripts/measure_linux_rss.py` using `/usr/bin/time -f %M`; 1 warm-up + 10 measurements per operation. Wall times below include the external `time` wrapper and are not comparable with direct subprocess latency above.

| Operation | Runs | Median peak RSS | Maximum peak RSS | Median wrapper-inclusive wall time |
|---|---:|---:|---:|---:|
| `version` | 10 | 4352.0 KiB | 4448.0 KiB | 2.004 ms |
| `doctor` | 10 | 6222.0 KiB | 6328.0 KiB | 3.967 ms |
| `files_list` | 10 | 6360.0 KiB | 6464.0 KiB | 4.337 ms |
| `config_show` | 10 | 6384.0 KiB | 6496.0 KiB | 4.527 ms |

## Not measured / not implied

- No installation time or first-download/toolchain setup time was measured.
- No truly cold-cache or cold-machine startup was measured.
- No macOS or Windows numbers were collected.
- No TypeScript baseline on identical hardware was collected; there is no evidence here for an across-product speed ranking.
- These runtime benchmarks do not prove the unimplemented execution sandbox, provider quality, feature parity, or general public-release readiness.
