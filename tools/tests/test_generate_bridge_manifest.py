from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path


TOOLS_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS_DIR))

import generate_bridge_manifest as generator  # noqa: E402


class BridgeManifestTests(unittest.TestCase):
    def test_parses_and_validates_exact_ewram_symbol(self) -> None:
        symbols = generator.parse_nm_symbols(
            "zero_sized t 080000F0\n"
            "gCoopNetBridge B 020376BC 241C\n"
            "other_symbol T 08000100 20\n"
        )

        bridge = generator.validate_bridge_symbol(symbols)

        self.assertEqual(bridge.address, 0x020376BC)
        self.assertEqual(bridge.size, 9244)
        self.assertEqual(symbols["zero_sized"].size, 0)

    def test_rejects_missing_duplicate_wrong_size_and_non_ewram_symbols(self) -> None:
        with self.assertRaises(generator.ManifestError):
            generator.validate_bridge_symbol({})
        with self.assertRaises(generator.ManifestError):
            generator.parse_nm_symbols(
                "gCoopNetBridge B 02000000 241C\n"
                "gCoopNetBridge B 0200241C 241C\n"
            )
        with self.assertRaises(generator.ManifestError):
            generator.validate_bridge_symbol(
                generator.parse_nm_symbols("gCoopNetBridge B 02000000 241B\n")
            )
        with self.assertRaises(generator.ManifestError):
            generator.validate_bridge_symbol(
                generator.parse_nm_symbols("gCoopNetBridge B 08000000 241C\n")
            )

    def test_manifest_and_lua_pin_the_same_layout(self) -> None:
        bridge = generator.Symbol("gCoopNetBridge", 0x020376BC, 9244, "B")
        digest = "ab" * 32

        manifest = generator.build_manifest(bridge, digest)
        lua = generator.render_lua(manifest)

        self.assertEqual(manifest["net_bridge"]["message"]["size"], 144)
        self.assertEqual(manifest["net_bridge"]["queue"]["size"], 4612)
        self.assertEqual(manifest["net_bridge"]["offsets"]["network_to_game"], 4632)
        self.assertIn("address = 0x020376BC", lua)
        self.assertIn("checksum = 140", lua)
        self.assertIn(digest, lua)

    def test_writes_deterministic_json_and_hashes_rom(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rom = root / "game.gba"
            rom.write_bytes(b"legal-local-build-fixture")
            digest = hashlib.sha256(rom.read_bytes()).hexdigest()
            manifest = generator.build_manifest(
                generator.Symbol("gCoopNetBridge", 0x020376BC, 9244, "B"), digest
            )
            output = root / "nested" / "bridge.json"

            generator.atomic_write(output, json.dumps(manifest, indent=2, sort_keys=True) + "\n")

            self.assertEqual(json.loads(output.read_text(encoding="utf-8")), manifest)
            self.assertEqual(generator.sha256_file(rom), digest)

    def test_rejects_noncanonical_digest(self) -> None:
        bridge = generator.Symbol("gCoopNetBridge", 0x020376BC, 9244, "B")
        for digest in ("AB" * 32, "a" * 63, "not-a-digest"):
            with self.subTest(digest=digest), self.assertRaises(generator.ManifestError):
                generator.build_manifest(bridge, digest)


if __name__ == "__main__":
    unittest.main()
