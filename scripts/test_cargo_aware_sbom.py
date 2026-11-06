#!/usr/bin/env python3
"""Network-free tests for Cargo-aware CycloneDX validation and orchestration."""
from __future__ import annotations

import contextlib
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from cargo_aware_sbom import main as cargo_sbom_main, validate_bom


def sample_bom() -> dict:
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "metadata": {"component": {"type": "application", "name": "docode-cli", "version": "0.7.0-rs.1"}},
        "components": [
            {"type": "library", "name": "serde", "version": "1.0.0", "licenses": [{"license": {"id": "MIT"}}]},
            {"type": "library", "name": "tokio", "version": "1.0.0"},
        ],
    }


class CargoSbomValidatorTests(unittest.TestCase):
    def test_valid_bom_reports_license_field_coverage_not_legal_clearance(self):
        result = validate_bom(sample_bom())
        self.assertEqual(result["bom_format"], "CycloneDX")
        self.assertEqual(result["dependency_components"], 2)
        self.assertEqual(result["components_with_license_fields"], 1)
        self.assertEqual(result["components_without_license_fields"], 1)
        self.assertEqual(result["license_coverage_percent"], 50.0)
        self.assertTrue(any("not a legal compatibility review" in x for x in result["important_limitations"]))

    def test_rejects_wrong_format(self):
        bom = sample_bom(); bom["bomFormat"] = "SPDX"
        with self.assertRaisesRegex(ValueError, "bomFormat"):
            validate_bom(bom)

    def test_rejects_wrong_version(self):
        bom = sample_bom(); bom["specVersion"] = "1.4"
        with self.assertRaisesRegex(ValueError, "specVersion"):
            validate_bom(bom)

    def test_rejects_missing_root_identity(self):
        bom = sample_bom(); bom["metadata"]["component"].pop("version")
        with self.assertRaisesRegex(ValueError, "root component"):
            validate_bom(bom)

    def test_rejects_empty_components(self):
        bom = sample_bom(); bom["components"] = []
        with self.assertRaisesRegex(ValueError, "non-empty"):
            validate_bom(bom)

    def test_rejects_component_missing_version(self):
        bom = sample_bom(); bom["components"][0].pop("version")
        with self.assertRaisesRegex(ValueError, "name/version"):
            validate_bom(bom)

    def test_rejects_non_object(self):
        with self.assertRaisesRegex(ValueError, "top-level"):
            validate_bom([])

    def test_orchestrator_moves_and_reports_validated_sbom_without_leaving_temp_file(self):
        # Mock only the external Cargo/plugin subprocess: this proves our wrapper's
        # orchestration/validation/artifact logic, not that cargo-cyclonedx ran.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nname="docode-cli"\nversion="0.7.0-rs.1"\n', encoding="utf-8")
            (root / "Cargo.lock").write_text('# synthetic lockfile for wrapper unit test\n', encoding="utf-8")
            output = root / "test-results" / "sbom" / "docode-cli-cargo-aware.cdx.json"
            report = root / "test-results" / "sbom" / "report.json"
            argv = ["cargo_aware_sbom.py", "--root", str(root), "--output", str(output), "--report", str(report),
                    "--cargo", "cargo", "--toolchain", "1.99.1"]

            def fake_run(command, *, cwd, timeout):
                if "cyclonedx" in command:
                    (Path(cwd) / "docode-cli-cargo-aware-sbom.json.json").write_text(json.dumps(sample_bom()), encoding="utf-8")
                return {"command": command, "exit_code": 0, "elapsed_seconds": 0.001, "output_tail": "mocked for unit test"}

            stdout = io.StringIO()
            stderr = io.StringIO()
            with patch.object(sys, "argv", argv), patch("cargo_aware_sbom.run", side_effect=fake_run), \
                 contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                result = cargo_sbom_main()
            self.assertEqual(result, 0, stderr.getvalue())
            self.assertTrue(output.is_file())
            self.assertTrue(report.is_file())
            self.assertFalse((root / "docode-cli-cargo-aware-sbom.json").exists())
            self.assertFalse((root / "docode-cli-cargo-aware-sbom.json.json").exists())
            saved_report = json.loads(report.read_text(encoding="utf-8"))
            self.assertEqual(saved_report["status"], "PASS")
            self.assertEqual(saved_report["generator"]["version"], "0.5.9")
            self.assertEqual(saved_report["validation"]["dependency_components"], 2)
            self.assertTrue(any("--spec-version" in item["command"] for item in saved_report["steps"]))


if __name__ == "__main__":
    unittest.main(verbosity=2)
