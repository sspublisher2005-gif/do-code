#!/usr/bin/env python3
"""Offline tests for the preliminary Cargo.lock → CycloneDX SBOM generator."""
from __future__ import annotations
import json
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("generate_sbom.py").resolve()
ROOT = SCRIPT.parent.parent


class SbomGeneratorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
        cls.manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))

    def generate(self, output: Path) -> dict:
        process = subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(ROOT), "--output", str(output)],
            capture_output=True, text=True, check=False, timeout=30,
        )
        self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
        self.assertTrue(output.is_file())
        return json.loads(output.read_text(encoding="utf-8"))

    def test_emits_valid_json_and_cyclonedx_header(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bom = self.generate(Path(temp) / "bom.json")
        self.assertEqual(bom["bomFormat"], "CycloneDX")
        self.assertEqual(bom["specVersion"], "1.5")
        self.assertRegex(bom["serialNumber"], r"^urn:uuid:[0-9a-f-]{36}$")
        self.assertEqual(bom["metadata"]["component"]["name"], "docode-cli")

    def test_locked_component_count_matches_lockfile(self) -> None:
        expected = sum(1 for package in self.lock["package"]
                       if not (package["name"] == self.manifest["package"]["name"]
                               and package["version"] == self.manifest["package"]["version"]
                               and "source" not in package))
        with tempfile.TemporaryDirectory() as temp:
            bom = self.generate(Path(temp) / "bom.json")
        self.assertEqual(len(bom["components"]), expected)

    def test_every_registry_component_has_locked_sha256(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bom = self.generate(Path(temp) / "bom.json")
        checksums = { (p["name"], p["version"]): p.get("checksum") for p in self.lock["package"] }
        for component in bom["components"]:
            checksum = checksums[(component["name"], component["version"])]
            if checksum:
                self.assertIn("hashes", component)
                self.assertEqual(component["hashes"][0]["content"], checksum)
                self.assertRegex(checksum, r"^[a-f0-9]{64}$")

    def test_dependency_references_are_internal_and_complete(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bom = self.generate(Path(temp) / "bom.json")
        refs = {bom["metadata"]["component"]["bom-ref"]} | {c["bom-ref"] for c in bom["components"]}
        self.assertEqual(len(refs), len(bom["components"]) + 1)
        self.assertTrue(all(edge["ref"] in refs for edge in bom["dependencies"]))
        self.assertTrue(all(dep in refs for edge in bom["dependencies"] for dep in edge.get("dependsOn", [])))
        self.assertEqual(len(bom["dependencies"]), len(refs))

    def test_license_claims_are_not_fabricated(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            bom = self.generate(Path(temp) / "bom.json")
        self.assertFalse(any("licenses" in component for component in bom["components"]))
        properties = {p["name"]: p["value"] for p in bom["metadata"]["properties"]}
        scope = properties["docode:sbom-scope"].lower()
        self.assertIn("licenses intentionally not asserted", scope)
        self.assertIn("checksums", scope)


if __name__ == "__main__":
    unittest.main(verbosity=2)
