"""Capture/cache integrity checks; no emulator or server is launched."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_author_fixtures as author
from live_harness_oracles import OracleFailure, read_flash


class AuthorFixtureTests(unittest.TestCase):
    recipe = {"abi": "hoenn-box80-v1", "party_species": [1, 2, 3, 4, 5, 6],
              "party_level": 5, "pc_species": [7], "bag_items": [[2, 3]]}

    def test_bad_recipe_is_rejected_before_ui(self):
        author.recipe_check(self.recipe)
        for change in ({"bag_items": [[]]}, {"bag_items": [[2, True]]},
                       {"bag_items": [[3, 3]]}, {"party_level": 101},
                       {"pc_species": []}, {"abi": "unrelated"}):
            with self.subTest(change=change), self.assertRaises(OracleFailure):
                author.recipe_check(dict(self.recipe, **change))

    def test_capture_retains_same_read_despite_source_replacement(self):
        fixture = ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav"
        data, generation = fixture.read_bytes(), read_flash(fixture).generation
        with tempfile.TemporaryDirectory() as directory:
            source, target = Path(directory) / "live.sav", Path(directory) / "retained.sav"
            source.write_bytes(data)
            receipt = {"save_sha256": author.hashlib.sha256(data).hexdigest()}
            def validate(*args):
                source.write_bytes(b"later ROM flush")
                return receipt
            with mock.patch.object(author, "check_population", side_effect=validate):
                result = author.retain_population(source.read_bytes(), target, b"", {}, generation)
                self.assertEqual(result, receipt)
                self.assertEqual(target.read_bytes(), data)
                with self.assertRaises(FileExistsError):
                    author.retain_population(data, target, b"", {}, generation)

    def test_failed_validation_and_generation_never_publish_save(self):
        fixture = ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav"
        data, generation = fixture.read_bytes(), read_flash(fixture).generation
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "population.sav"
            with self.assertRaisesRegex(OracleFailure, "generation"):
                author.retain_population(data, target, b"", {}, generation + 1)
            with mock.patch.object(author, "check_population", side_effect=OracleFailure("bad population")):
                with self.assertRaisesRegex(OracleFailure, "bad population"):
                    author.retain_population(data, target, b"", {}, generation)
            self.assertFalse(target.exists())

    def test_seed_replacement_does_not_change_validated_buffer(self):
        fixture = ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav"
        data = fixture.read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            seed = Path(directory) / "seed.sav"
            seed.write_bytes(data)
            initial, source = author.seed_bytes(seed, author.hashlib.sha256(data).hexdigest())
            seed.write_bytes(b"replacement")
            sandbox = Path(directory) / "game.sav"
            sandbox.write_bytes(initial)
            self.assertEqual(sandbox.read_bytes(), data)
            with self.assertRaisesRegex(OracleFailure, "seed bytes"):
                author.seed_bytes(seed, source.sha256)
            with self.assertRaisesRegex(OracleFailure, "lineage"):
                author.retain_population(initial, Path(directory) / "population.sav", b"", {}, source.generation, b"wrong trainer")

    def test_early_boundary_closes_only_owned_process_and_preserves_error(self):
        for boundary in ("startup", "input"):
            with self.subTest(boundary=boundary), tempfile.TemporaryDirectory() as directory:
                process = mock.Mock()
                process.wait.side_effect = [author.subprocess.TimeoutExpired("owned", 10), None]
                with mock.patch.object(author.subprocess, "Popen", return_value=process):
                    with self.assertRaisesRegex(RuntimeError, boundary):
                        with author.owned_emulator(Path("mgba.exe"), Path(directory), {}, "author"):
                            raise RuntimeError(boundary)
                process.terminate.assert_called_once()
                process.kill.assert_called_once()
                self.assertEqual(process.wait.call_args_list, [mock.call(timeout=10), mock.call(timeout=10)])
        with tempfile.TemporaryDirectory() as directory:
            process = mock.Mock()
            process.terminate.side_effect = OSError("cleanup")
            with mock.patch.object(author.subprocess, "Popen", return_value=process):
                with self.assertRaisesRegex(RuntimeError, "primary") as caught:
                    with author.owned_emulator(Path("mgba.exe"), Path(directory), {}, "author"):
                        raise RuntimeError("primary")
            self.assertIn("cleanup", caught.exception.__notes__[0])

    def test_reuse_rejects_changed_save_inputs_and_cold_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs, population = {"rom": "pinned"}, {"save_sha256": "captured"}
            (root / "population.sav").write_bytes(b"unit fixture")
            (root / "cold-party.png").write_bytes(b"unit screenshot")
            receipt = {"inputs": inputs, "cache_key": author.cache_key(inputs),
                       "population": population, "cold_save_sha256": "captured",
                       "screenshots": {"cold-party.png": author.harness.digest(root / "cold-party.png")}}
            def publish():
                (root / "receipt.json").write_text(json.dumps(receipt))
            publish()
            with mock.patch.object(author, "read_flash_bytes"), mock.patch.object(author, "check_population", return_value=population):
                self.assertEqual(author.cached_receipt(root, inputs, b"", {}), receipt)
                with self.assertRaisesRegex(OracleFailure, "inputs"):
                    author.cached_receipt(root, {"rom": "different"}, b"", {})
                receipt["cold_save_sha256"] = "wrong"
                publish()
                with self.assertRaisesRegex(OracleFailure, "cold"):
                    author.cached_receipt(root, inputs, b"", {})
                receipt["cold_save_sha256"] = "captured"
                publish()
                (root / "cold-party.png").write_bytes(b"changed")
                with self.assertRaisesRegex(OracleFailure, "screenshot"):
                    author.cached_receipt(root, inputs, b"", {})
                publish()
                with mock.patch.object(author, "check_population", return_value={"save_sha256": "changed"}):
                    with self.assertRaisesRegex(OracleFailure, "population"):
                        author.cached_receipt(root, inputs, b"", {})


if __name__ == "__main__":
    unittest.main()
