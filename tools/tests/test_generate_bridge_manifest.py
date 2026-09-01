from __future__ import annotations

import hashlib
import json
import re
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


TOOLS_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = TOOLS_DIR.parent
sys.path.insert(0, str(TOOLS_DIR))

import generate_bridge_manifest as generator  # noqa: E402


REGISTRY = {
    "registry_version": 1,
    "identities": [
        {"ordinal": 0, "id": "HOENN:TRAINER_WALLY_1", "kind": "trainer"},
        {"ordinal": 0, "id": "HOENN:BADGE_STONE", "kind": "gym"},
    ],
}


def registry_digest(value: object = REGISTRY) -> bytes:
    canonical = json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=True,
    ).encode("ascii")
    return hashlib.sha256(canonical).digest()[:16]


def descriptor_bytes(**overrides: object) -> bytes:
    fields: dict[str, object] = {
        "descriptor_magic": 0x31445343,
        "descriptor_version": 1,
        "descriptor_size": 64,
        "save_magic": 0x31505343,
        "save_schema_version": 1,
        "save_struct_size": 672,
        "save_block3_offset": 4,
        "generation_offset": 28,
        "crc32_offset": 668,
        "sector_data_size": 3968,
        "save_block3_chunk_size": 116,
        "sector_size": 4096,
        "sectors_per_slot": 15,
        "save_slot_count": 2,
        "registry_version": 1,
        "registry_digest": registry_digest(),
        "trainer_bits_offset": 68,
        "event_bits_offset": 324,
        "fly_bits_offset": 580,
        "gym_bits_offset": 596,
        "status_flags_offset": 32,
        "regional_progress_offset": 36,
    }
    fields.update(overrides)
    return struct.pack(
        "<IHHI10HI16s6H",
        fields["descriptor_magic"],
        fields["descriptor_version"],
        fields["descriptor_size"],
        fields["save_magic"],
        fields["save_schema_version"],
        fields["save_struct_size"],
        fields["save_block3_offset"],
        fields["generation_offset"],
        fields["crc32_offset"],
        fields["sector_data_size"],
        fields["save_block3_chunk_size"],
        fields["sector_size"],
        fields["sectors_per_slot"],
        fields["save_slot_count"],
        fields["registry_version"],
        fields["registry_digest"],
        fields["trainer_bits_offset"],
        fields["event_bits_offset"],
        fields["fly_bits_offset"],
        fields["gym_bits_offset"],
        fields["status_flags_offset"],
        fields["regional_progress_offset"],
    )


def valid_symbols() -> dict[str, generator.Symbol]:
    return generator.parse_nm_symbols(
        "gCoopNetBridge B 020376BC 241C\n"
        "gSaveblock3 B 02000100 2A4\n"
        "gCoopSaveSchemaDescriptor R 08000100 40\n"
    )


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

    def test_rejects_missing_duplicate_wrong_size_and_non_ewram_bridge(self) -> None:
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

    def test_validates_save_symbols_and_rejects_ambiguous_or_unsafe_layouts(self) -> None:
        block3, descriptor = generator.validate_save_symbols(valid_symbols())
        self.assertEqual(block3.address, 0x02000100)
        self.assertEqual(block3.size, 676)
        self.assertEqual(descriptor.address, 0x08000100)

        malformed_records = (
            "gSaveblock3 B 02000100 2A4\n",
            "gCoopSaveSchemaDescriptor R 08000100 40\n",
            (
                "gSaveblock3 B 02000100 2A4\n"
                "gSaveblock3 B 020003A4 2A4\n"
                "gCoopSaveSchemaDescriptor R 08000100 40\n"
            ),
            (
                "gSaveblock3 B 02000100 2A3\n"
                "gCoopSaveSchemaDescriptor R 08000100 40\n"
            ),
            (
                "gSaveblock3 B 02000100 2B9\n"
                "gCoopSaveSchemaDescriptor R 08000100 40\n"
            ),
            (
                "gSaveblock3 B FFFFFFF0 2A4\n"
                "gCoopSaveSchemaDescriptor R 08000100 40\n"
            ),
            (
                "gSaveblock3 B 02000102 2A4\n"
                "gCoopSaveSchemaDescriptor R 08000100 40\n"
            ),
            (
                "gSaveblock3 B 02000100 2A4\n"
                "gCoopSaveSchemaDescriptor R 08000100 3F\n"
            ),
            (
                "gSaveblock3 B 02000100 2A4\n"
                "gCoopSaveSchemaDescriptor R 02000200 40\n"
            ),
            (
                "gSaveblock3 B 02000100 2A4\n"
                "gCoopSaveSchemaDescriptor R 08000102 40\n"
            ),
        )
        for records in malformed_records:
            with self.subTest(records=records), self.assertRaises(generator.ManifestError):
                generator.validate_save_symbols(generator.parse_nm_symbols(records))

    def test_reads_descriptor_from_checked_rom_symbol_bounds(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            rom = Path(directory) / "game.gba"
            rom.write_bytes(b"\xFF" * 0x100 + descriptor_bytes() + b"tail")
            descriptor_symbol = valid_symbols()["gCoopSaveSchemaDescriptor"]

            payload = generator.read_save_descriptor_bytes(rom, descriptor_symbol)

            self.assertEqual(payload, descriptor_bytes())

            truncated = generator.Symbol(
                descriptor_symbol.name,
                0x08000120,
                descriptor_symbol.size,
                descriptor_symbol.kind,
            )
            with self.assertRaises(generator.ManifestError):
                generator.read_save_descriptor_bytes(rom, truncated)
            with self.assertRaises(generator.ManifestError):
                generator.read_save_descriptor_bytes(Path(directory), descriptor_symbol)

    def test_validates_every_descriptor_field_and_registry_identity(self) -> None:
        registry = generator.registry_contract(REGISTRY)
        parsed = generator.parse_save_descriptor(descriptor_bytes())
        generator.validate_save_descriptor(parsed, registry)

        malformed = {
            "descriptor_magic": 0,
            "descriptor_version": 2,
            "descriptor_size": 63,
            "save_magic": 0,
            "save_schema_version": 2,
            "save_struct_size": 671,
            "save_block3_offset": 5,
            "generation_offset": 27,
            "crc32_offset": 667,
            "sector_data_size": 3967,
            "save_block3_chunk_size": 115,
            "sector_size": 4095,
            "sectors_per_slot": 14,
            "save_slot_count": 1,
            "registry_version": 2,
            "registry_digest": b"x" * 16,
            "trainer_bits_offset": 69,
            "event_bits_offset": 325,
            "fly_bits_offset": 581,
            "gym_bits_offset": 597,
            "status_flags_offset": 33,
            "regional_progress_offset": 37,
        }
        for field, value in malformed.items():
            with self.subTest(field=field), self.assertRaises(generator.ManifestError):
                generator.validate_save_descriptor(
                    generator.parse_save_descriptor(descriptor_bytes(**{field: value})),
                    registry,
                )

        for payload in (b"", descriptor_bytes()[:-1], descriptor_bytes() + b"x"):
            with self.subTest(size=len(payload)), self.assertRaises(generator.ManifestError):
                generator.parse_save_descriptor(payload)

    def test_registry_contract_uses_canonical_ascii_json(self) -> None:
        contract = generator.registry_contract(REGISTRY)
        self.assertEqual(contract.version, 1)
        self.assertEqual(contract.digest, registry_digest())
        self.assertEqual(contract.digest.hex(), registry_digest().hex())

        for malformed in (
            [],
            {},
            {"registry_version": True},
            {"registry_version": 0},
            {"registry_version": 2},
            {"registry_version": 1, "invalid": float("nan")},
        ):
            with self.subTest(value=malformed), self.assertRaises(generator.ManifestError):
                generator.registry_contract(malformed)

    def test_repository_registry_matches_generated_c_digest(self) -> None:
        contract = generator.load_registry_contract(
            REPO_ROOT / "data" / "coop" / "regional_identities.json"
        )
        header = (REPO_ROOT / "include" / "coop" / "generated_regional_identities.h").read_text(
            encoding="utf-8"
        )
        match = re.search(
            r"#define COOP_IDENTITY_REGISTRY_DIGEST_BYTES \{ ([^}]+) \}",
            header,
        )
        self.assertIsNotNone(match)
        assert match is not None
        generated = bytes(int(token.strip(), 16) for token in match.group(1).split(","))

        self.assertEqual(contract.version, 1)
        self.assertEqual(generated, contract.digest)

    def test_manifest_and_lua_pin_bridge_save_and_registry_layouts(self) -> None:
        symbols = valid_symbols()
        bridge = generator.validate_bridge_symbol(symbols)
        block3, _ = generator.validate_save_symbols(symbols)
        registry = generator.registry_contract(REGISTRY)
        descriptor = generator.parse_save_descriptor(descriptor_bytes())
        digest = "ab" * 32

        manifest = generator.build_manifest(bridge, block3, descriptor, registry, digest)
        lua = generator.render_lua(manifest)

        self.assertEqual(manifest["schema_version"], 2)
        self.assertEqual(
            manifest["game_build"]["id"],
            "pokecrossroads-beta-1.4-e05c8286-coop-v1",
        )
        self.assertEqual(manifest["net_bridge"]["message"]["size"], 144)
        self.assertEqual(manifest["net_bridge"]["queue"]["size"], 4612)
        self.assertEqual(manifest["net_bridge"]["offsets"]["network_to_game"], 4632)
        self.assertEqual(
            manifest["save"],
            {
                "block3_address": 0x02000100,
                "coop_offset": 4,
                "generation_offset": 28,
                "generation_address": 0x02000120,
                "crc_offset": 668,
                "schema_version": 1,
                "struct_size": 672,
                "registry_version": 1,
                "registry_digest": registry_digest().hex(),
            },
        )
        self.assertIn("address = 0x020376BC", lua)
        self.assertIn("checksum = 140", lua)
        self.assertIn("save = {", lua)
        self.assertIn("block3_address = 0x02000100", lua)
        self.assertIn("generation_address = 0x02000120", lua)
        self.assertIn(f'registry_digest = "{registry_digest().hex()}"', lua)
        self.assertIn(digest, lua)
        self.assertNotIn(manifest["game_build"]["id"], lua)

    def test_manifest_rechecks_descriptor_and_block_bounds(self) -> None:
        symbols = valid_symbols()
        bridge = generator.validate_bridge_symbol(symbols)
        block3, _ = generator.validate_save_symbols(symbols)
        registry = generator.registry_contract(REGISTRY)

        with self.assertRaises(generator.ManifestError):
            generator.build_manifest(
                bridge,
                block3,
                generator.parse_save_descriptor(descriptor_bytes(registry_version=2)),
                registry,
                "ab" * 32,
            )
        with self.assertRaises(generator.ManifestError):
            generator.build_manifest(
                bridge,
                generator.Symbol("gSaveblock3", 0x0203FF00, 676, "B"),
                generator.parse_save_descriptor(descriptor_bytes()),
                registry,
                "ab" * 32,
            )

    def test_writes_deterministic_json_and_hashes_rom(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rom = root / "game.gba"
            rom.write_bytes(b"legal-local-build-fixture")
            digest = hashlib.sha256(rom.read_bytes()).hexdigest()
            symbols = valid_symbols()
            block3, _ = generator.validate_save_symbols(symbols)
            manifest = generator.build_manifest(
                generator.validate_bridge_symbol(symbols),
                block3,
                generator.parse_save_descriptor(descriptor_bytes()),
                generator.registry_contract(REGISTRY),
                digest,
            )
            output = root / "nested" / "bridge.json"

            generator.atomic_write(output, json.dumps(manifest, indent=2, sort_keys=True) + "\n")

            self.assertEqual(json.loads(output.read_text(encoding="utf-8")), manifest)
            self.assertEqual(generator.sha256_file(rom), digest)

    def test_rejects_noncanonical_digest(self) -> None:
        symbols = valid_symbols()
        bridge = generator.validate_bridge_symbol(symbols)
        block3, _ = generator.validate_save_symbols(symbols)
        descriptor = generator.parse_save_descriptor(descriptor_bytes())
        registry = generator.registry_contract(REGISTRY)
        for digest in ("AB" * 32, "a" * 63, "not-a-digest", None):
            with self.subTest(digest=digest), self.assertRaises(generator.ManifestError):
                generator.build_manifest(bridge, block3, descriptor, registry, digest)

    def test_rejects_invalid_configured_textual_build_ids(self) -> None:
        symbols = valid_symbols()
        bridge = generator.validate_bridge_symbol(symbols)
        block3, _ = generator.validate_save_symbols(symbols)
        descriptor = generator.parse_save_descriptor(descriptor_bytes())
        registry = generator.registry_contract(REGISTRY)
        for build_id in ("", "a" * 129, "pokecrossroads-é", "has space", "has/slash"):
            with (
                self.subTest(build_id=build_id),
                mock.patch.object(generator, "GAME_BUILD_TEXT_ID", build_id),
                self.assertRaises(generator.ManifestError),
            ):
                generator.build_manifest(bridge, block3, descriptor, registry, "ab" * 32)

    def test_accepts_maximum_length_textual_build_id(self) -> None:
        self.assertEqual(generator.validate_game_build_id("A" * 128), "A" * 128)

    def test_checked_in_manifest_requires_a_fresh_linked_rom_for_schema_two(self) -> None:
        manifest_path = REPO_ROOT / "dist" / "bridge_manifest.json"
        checked_in_text = manifest_path.read_text(encoding="utf-8")
        checked_in = json.loads(checked_in_text)

        # A schema-v1 manifest remains a pinned, real build artifact. It must not
        # be rewritten with invented linked addresses when no ELF/ROM is present.
        self.assertIn(checked_in["schema_version"], (1, generator.MANIFEST_SCHEMA_VERSION))
        if checked_in["schema_version"] == generator.MANIFEST_SCHEMA_VERSION:
            registry = generator.load_registry_contract(
                REPO_ROOT / "data" / "coop" / "regional_identities.json"
            )
            expected = generator.build_manifest(
                generator.Symbol(
                    "gCoopNetBridge",
                    checked_in["net_bridge"]["address"],
                    checked_in["net_bridge"]["size"],
                    "B",
                ),
                generator.Symbol(
                    "gSaveblock3",
                    checked_in["save"]["block3_address"],
                    676,
                    "B",
                ),
                generator.parse_save_descriptor(
                    descriptor_bytes(registry_digest=registry.digest)
                ),
                registry,
                checked_in["game_build"]["rom_sha256"],
            )
            self.assertEqual(checked_in, expected)
            self.assertEqual(
                checked_in_text,
                json.dumps(expected, indent=2, sort_keys=True) + "\n",
            )
            self.assertEqual(
                (REPO_ROOT / "bridge" / "generated_addresses.lua").read_text(
                    encoding="utf-8"
                ),
                generator.render_lua(expected),
            )

    def test_example_lua_documents_schema_two_save_contract(self) -> None:
        example = (REPO_ROOT / "bridge" / "generated_addresses.lua.example").read_text(
            encoding="utf-8"
        )
        for expected in (
            "schema_version = 2",
            "save = {",
            "coop_offset = 4",
            "generation_offset = 28",
            "generation_address = 0x02000020",
            "crc_offset = 668",
            "struct_size = 672",
            "registry_version = 1",
            'registry_digest = string.rep("0", 32)',
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, example)


if __name__ == "__main__":
    unittest.main()
