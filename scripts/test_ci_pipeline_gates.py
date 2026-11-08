#!/usr/bin/env python3
"""Regression tests: CI must not overlook failed, missing, or duplicate release gates."""
from __future__ import annotations

import unittest

from ci_pipeline import CORE_REQUIRED_GATES, evaluate_release_gates


def passing_steps() -> list[dict[str, str]]:
    return [{"name": name, "status": "PASS"} for name in sorted(CORE_REQUIRED_GATES)]


class ReleaseGateEvaluatorTests(unittest.TestCase):
    def test_all_core_gates_pass_without_failure(self):
        self.assertEqual(evaluate_release_gates(passing_steps(), require_cargo_aware_sbom=False,
                                                require_linux_rss=False), [])

    def test_missing_core_gate_fails(self):
        steps = [s for s in passing_steps() if s["name"] != "cargo test --all-targets --locked"]
        failures = evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=False)
        self.assertTrue(any(f["name"] == "cargo test --all-targets --locked" and f["status"] == "MISSING" for f in failures))

    def test_python_harness_syntax_gate_is_required(self):
        steps = [s for s in passing_steps() if s["name"] != "Python harness syntax"]
        failures = evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=False)
        self.assertTrue(any(f["name"] == "Python harness syntax" and f["status"] == "MISSING" for f in failures))

    def test_failing_test_gate_is_detected_by_exact_current_name(self):
        steps = passing_steps()
        next(s for s in steps if s["name"] == "cargo test --all-targets --locked")["status"] = "FAIL"
        failures = evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=False)
        self.assertTrue(any(f["name"] == "cargo test --all-targets --locked" and f["status"] == "FAIL" for f in failures))

    def test_cargo_aware_sbom_is_required_only_on_configured_job(self):
        steps = passing_steps()
        steps.append({"name": "Generate Cargo-aware CycloneDX SBOM", "status": "NOT_APPLICABLE"})
        self.assertEqual(evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=False), [])
        failures = evaluate_release_gates(steps, require_cargo_aware_sbom=True, require_linux_rss=False)
        self.assertTrue(any(f["name"] == "Generate Cargo-aware CycloneDX SBOM" and f["status"] == "NOT_APPLICABLE" for f in failures))

    def test_linux_rss_gate_is_required_only_on_linux(self):
        steps = passing_steps()
        self.assertEqual(evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=False), [])
        failures = evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=True)
        self.assertTrue(any(f["name"] == "Linux peak-RSS diagnostic" and f["status"] == "MISSING" for f in failures))

    def test_duplicate_step_names_fail_closed(self):
        steps = passing_steps()
        steps.append({"name": "cargo test --all-targets --locked", "status": "PASS"})
        failures = evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=False)
        self.assertTrue(any(f["name"] == "cargo test --all-targets --locked" and f["status"] == "DUPLICATE" for f in failures))

    def test_legacy_wrong_label_does_not_satisfy_current_gate(self):
        steps = [s for s in passing_steps() if s["name"] != "cargo test --all-targets --locked"]
        steps.append({"name": "cargo test --locked (unit + integration)", "status": "PASS"})
        failures = evaluate_release_gates(steps, require_cargo_aware_sbom=False, require_linux_rss=False)
        self.assertTrue(any(f["name"] == "cargo test --all-targets --locked" and f["status"] == "MISSING" for f in failures))


if __name__ == "__main__":
    unittest.main(verbosity=2)
