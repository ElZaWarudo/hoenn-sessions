"""Mocked phase, custody composition, per-session pairing-code groups, and cleanup boundaries."""
from contextlib import ExitStack
import copy
import base64
import hashlib
import inspect
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_roundtrip as rt


def base():
    return {"release_dir": "release", "run_dir": "journey", "custody_plan": {},
            "players": [{"name": n, "character_id": n, "source_sha256": hashlib.sha256(b"save").hexdigest(),
                         "profile_localappdata": "profile-" + n} for n in ("a", "b")],
            "legs": [{"name": "out", "source_world_id": 1}, {"name": "back"}]}


class RoundtripTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir="S:/cormoria-build" if Path("S:/cormoria-build").is_dir() else None)
        self.addCleanup(self.temp.cleanup); self.root = Path(self.temp.name)

    def lifecycle(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        m = {}
        m["inputs"] = stack.enter_context(mock.patch.object(rt, "inputs", return_value=(base(), {"input": "pinned"})))
        m["space"] = stack.enter_context(mock.patch.object(rt.harness, "check_c_space"))
        stack.enter_context(mock.patch.object(rt.harness, "preflight"))
        def author(path, plan, config, roots, root):
            return rt.plan_receipt(rt.immutable(root / "custody-plan.json", plan))
        def seed(plan, root, register):
            return rt.plan_receipt(rt.immutable(root / "signed-plan.json", plan), actors={"a": {}, "b": {}})
        def outbound(plan, actors, root):
            return rt.plan_receipt(rt.immutable(root / "attested-plan.json", plan))
        for name, callback in (("author", author), ("seed_plan", seed), ("outbound", outbound)):
            m[name] = stack.enter_context(mock.patch.object(rt, name, side_effect=callback))
        m["prepare"] = stack.enter_context(mock.patch.object(rt.prepare, "prepare", return_value={"reused": ["a", "b"]}))
        m["return"] = stack.enter_context(mock.patch.object(rt, "return_leg", return_value={"report": {"strict": True}}))
        m["complete"] = stack.enter_context(mock.patch.object(rt, "complete_check"))
        m["launch"] = stack.enter_context(mock.patch.object(rt.harness, "launch"))
        return m

    def run_mock(self):
        return rt.run(Path("base.json"), Path("config"), {"outputs": {}}, self.root)

    def test_completed_retry_skips_factory_seed_group_prepare_and_launch(self):
        m = self.lifecycle(); first = self.run_mock(); second = self.run_mock()
        self.assertFalse(first["reused"]); self.assertTrue(second["reused"])
        for name in ("author", "seed_plan", "prepare", "outbound", "return"):
            self.assertEqual(m[name].call_count, 1)
        m["launch"].assert_not_called()
        self.assertEqual(m["complete"].call_count, 2)

    def test_completed_retry_low_space_is_read_only_and_retains_input_checks(self):
        actual_space = rt.harness.check_c_space
        m = self.lifecycle(); first = self.run_mock()
        root = Path(first["plan_path"]).parent
        files = {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in root.rglob("*") if p.is_file()}
        m["space"].reset_mock(); m["space"].side_effect = actual_space
        with mock.patch.object(rt.harness.shutil, "disk_usage", return_value=SimpleNamespace(free=1)), \
                mock.patch.object(rt.groups, "_call") as api:
            self.assertTrue(self.run_mock()["reused"])
            m["space"].assert_not_called(); api.assert_not_called()
            self.assertEqual(files, {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in root.rglob("*") if p.is_file()})
            inputs_path = root / "inputs.json"
            original = inputs_path.read_bytes()
            inputs_path.write_bytes(b"changed")
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "completed inputs"):
                self.run_mock()
            self.assertEqual(inputs_path.read_bytes(), b"changed")
            inputs_path.unlink()
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "completed inputs"):
                self.run_mock()
            self.assertFalse(inputs_path.exists())
            inputs_path.write_bytes(original)
        for label in ("author", "seed_plan", "prepare", "outbound", "return"):
            self.assertEqual(m[label].call_count, 1)
        m["launch"].assert_not_called()

    def test_fresh_and_incomplete_low_space_stop_before_any_new_file_or_effect(self):
        actual_space = rt.harness.check_c_space
        m = self.lifecycle()
        m["space"].side_effect = actual_space
        with mock.patch.object(rt.harness.shutil, "disk_usage", return_value=SimpleNamespace(free=1)), \
                mock.patch.object(rt.groups, "_call") as api:
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "below 2 GiB"):
                self.run_mock()
            self.assertEqual(list(self.root.iterdir()), [])
            for label in ("author", "seed_plan", "prepare", "outbound", "return", "launch"):
                m[label].assert_not_called()
            api.assert_not_called()
        m["space"].side_effect = None
        m["return"].side_effect = RuntimeError("partial crossing")
        with self.assertRaisesRegex(RuntimeError, "partial crossing"):
            self.run_mock()
        files = {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in self.root.rglob("*") if p.is_file()}
        calls = {label: m[label].call_count for label in ("author", "seed_plan", "prepare", "outbound", "return")}
        m["space"].side_effect = actual_space
        with mock.patch.object(rt.harness.shutil, "disk_usage", return_value=SimpleNamespace(free=1)), \
                mock.patch.object(rt.groups, "_call") as api:
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "below 2 GiB"):
                self.run_mock()
            self.assertEqual(files, {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in self.root.rglob("*") if p.is_file()})
            self.assertEqual(calls, {label: m[label].call_count for label in calls})
            api.assert_not_called(); m["launch"].assert_not_called()

    def test_interrupted_return_never_reseeds_or_replays_outbound(self):
        m = self.lifecycle(); m["return"].side_effect = RuntimeError("partial crossing")
        with self.assertRaisesRegex(RuntimeError, "partial crossing"): self.run_mock()
        with self.assertRaisesRegex(rt.harness.HarnessFailure, "ambiguous effects"): self.run_mock()
        self.assertEqual(m["seed_plan"].call_count, 1)
        self.assertEqual(m["outbound"].call_count, 1)
        self.assertEqual(m["return"].call_count, 1)

    def test_stale_phase_and_changed_immutable_inputs_rejected(self):
        rt.immutable(self.root / "prepared.json", {"input_key": "old", "phase": "prepared", "result": {}})
        action = mock.Mock()
        with self.assertRaisesRegex(rt.harness.HarnessFailure, "stale phase"):
            rt.phase(self.root, "current", "prepared", action)
        action.assert_not_called()
        with self.assertRaises(rt.harness.HarnessFailure): rt.immutable(self.root / "prepared.json", {"changed": True})

    def test_tampered_completed_plan_is_rejected_without_effects(self):
        m = self.lifecycle(); result = self.run_mock()
        m["space"].reset_mock(); m["space"].side_effect = rt.harness.HarnessFailure("below 2 GiB")
        Path(result["plan_path"]).write_text("tampered")
        with self.assertRaises(rt.harness.HarnessFailure): self.run_mock()
        self.assertEqual(m["seed_plan"].call_count, 1); self.assertEqual(m["outbound"].call_count, 1)
        m["space"].assert_not_called()

    def test_seed_ids_must_be_distinct_and_match_full_sources(self):
        p = base(); actors = {n: {"character_id": "same", "source_sha256": p["players"][0]["source_sha256"], "revision": 6} for n in ("a", "b")}
        with mock.patch.object(rt.seeder, "seed", return_value={"players": actors}):
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "duplicate"): rt.seed_plan(p, self.root, False)
            actors["b"]["character_id"] = "other"; actors["b"]["source_sha256"] = "wrong"
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "source differs"): rt.seed_plan(p, self.root, False)
        self.assertFalse((self.root / "signed-plan.json").exists())

    def test_cleanup_records_liveness_and_preserves_first_error(self):
        with mock.patch.object(rt.harness, "stop_runtime", side_effect=RuntimeError("stop")) as stop, \
                mock.patch.object(rt.harness, "close_desktops", side_effect=RuntimeError("close")) as close, \
                mock.patch.object(rt.harness.psutil, "pid_exists", return_value=True):
            original = RuntimeError("input failure")
            try: raise original
            except RuntimeError:
                rt.cleanup(base(), {"a": 10, "b": 11}, self.root / "cleanup.json")
            self.assertEqual(len(original.__notes__), 3)
            stop.assert_called_once(); close.assert_called_once()
            receipt = json.loads((self.root / "cleanup.json").read_text())
            self.assertFalse(receipt["ok"]); self.assertEqual(receipt["alive_pids"], [10, 11])

    def test_cleanup_failure_prevents_success(self):
        with mock.patch.object(rt.harness, "stop_runtime", side_effect=RuntimeError("stop")), \
                mock.patch.object(rt.harness, "close_desktops"), \
                mock.patch.object(rt.harness.psutil, "pid_exists", return_value=False):
            with self.assertRaisesRegex(RuntimeError, "stop"):
                rt.cleanup(base(), {"a": 10}, self.root / "cleanup.json")

    def test_ambiguous_outbound_pairing_phase_never_blindly_retries(self):
        action = mock.Mock(side_effect=RuntimeError("pairing uncertain"))
        with self.assertRaises(RuntimeError): rt.phase(self.root, "key", "outbound", action)
        with self.assertRaisesRegex(rt.harness.HarnessFailure, "ambiguous effects"):
            rt.phase(self.root, "key", "outbound", action)
        self.assertEqual(action.call_count, 1)

    def test_unsupported_factory_adapter_rejected_without_authoring_or_launch(self):
        with mock.patch.object(rt.harness, "_read_json", return_value=base()), \
                mock.patch.object(rt.harbor, "author_player") as author, mock.patch.object(rt.harness, "launch") as launch:
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "unsupported"):
                rt.inputs(Path("plan"), Path("config"), {"adapter": "unknown"}, False)
            author.assert_not_called(); launch.assert_not_called()

    def test_factory_composes_actual_population_source_before_mail_and_publisher(self):
        modules = (rt.harbor, rt.population, rt.mail, rt.pc, rt.third, rt.daycare)
        signatures = {module: inspect.signature(module.author_player) for module in modules}
        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(rt.harness, "check_c_space"))
            receipts = {}
            for module, label in ((rt.harbor, "harbor"), (rt.population, "population"), (rt.mail, "mail"),
                                  (rt.pc, "pc_mail"), (rt.third, "third_mail"), (rt.daycare, "daycare")):
                receipt = {"cache_key": label}
                if module is rt.population:
                    # Accepted population receipts wrap the oracle under validated.population.
                    receipt.update(seed_lineage=[{"path": "population.sav", "sha256": "actual-population"}],
                                   validated={"population": {"save_sha256": "actual-population",
                                                              "shared_witnesses": [{"field_id": 257}]},
                                              "identities": [], "source_sha256": "harbor-source", "lineage_hex": "identity"})
                receipts[label] = stack.enter_context(mock.patch.object(module, "author_player", return_value=receipt))
            def harbor_contract(plan, name, output, config):
                rt.harbor.controls(name)  # Real accepted helper contract; lower-case must fail.
                return {"cache_key": "harbor"}
            receipts["harbor"].side_effect = harbor_contract
            def publish(path, parents, config, output):
                derived = json.loads(path.read_text())
                self.assertTrue(all(p["source_sha256"] == "actual-population" for p in derived["players"]))
                self.assertTrue(all(p["shared_witnesses"] == [{"field_id": 257}] for p in derived["players"]))
                self.assertTrue(all(p["seed_lineage"] == receipts["population"].return_value["seed_lineage"] for p in derived["players"]))
                self.assertEqual(set(parents), {"a", "b"})
                return rt.immutable(output, derived)
            stack.enter_context(mock.patch.object(rt.publisher, "publish_plan", side_effect=publish))
            roots = {stage: str(self.root / stage) for stage in rt.STAGES}
            rt.author(Path("base"), base(), Path("config"), roots, self.root)
            for receipt in receipts.values(): self.assertEqual(receipt.call_count, 2)
            for module, label in zip(modules, rt.STAGES):
                for call in receipts[label].call_args_list:
                    signatures[module].bind(*call.args, **call.kwargs)
            self.assertEqual([call.args[1] for call in receipts["harbor"].call_args_list], ["A", "B"])
            for label in ("population", "pc_mail", "third_mail", "daycare"):
                self.assertEqual([call.args[1] for call in receipts[label].call_args_list], ["a", "b"])
            for call in receipts["population"].call_args_list:
                self.assertEqual(call.args[2], Path(roots["harbor"]) / "harbor")
            for call in receipts["pc_mail"].call_args_list:
                self.assertEqual(call.args[2], Path(roots["mail"]) / "mail")
            for call in receipts["third_mail"].call_args_list:
                self.assertEqual(call.args[2:4], (Path(roots["pc_mail"]) / "pc_mail", Path(roots["mail"]) / "mail"))
            for call in receipts["daycare"].call_args_list:
                self.assertEqual(call.args[2:5], (Path(roots["third_mail"]) / "third_mail",
                                                Path(roots["pc_mail"]) / "pc_mail", Path(roots["mail"]) / "mail"))
            for call in receipts["mail"].call_args_list:
                self.assertEqual(call.args[1]["source_sha256"], "actual-population")
                self.assertEqual(call.args[1]["shared_witnesses"], [{"field_id": 257}])
                self.assertEqual(call.args[1]["seed_lineage"], receipts["population"].return_value["seed_lineage"])
                self.assertTrue(call.args[1]["source_save"].endswith("population.sav"))

    def test_actual_harbor_entry_accepts_adapter_name_before_any_emulator(self):
        roots = {stage: str(self.root / stage) for stage in rt.STAGES}
        with mock.patch.object(rt.harness, "preflight"), mock.patch.object(rt.harness, "check_c_space"), \
                mock.patch.object(rt.harbor, "config_check", side_effect=RuntimeError("real controls accepted")), \
                mock.patch.object(rt.harbor, "owned_scripted_emulator") as emulator:
            with self.assertRaisesRegex(RuntimeError, "real controls accepted"):
                rt.author(Path("base"), base(), Path("config"), roots, self.root)
            emulator.assert_not_called()

    def test_completed_proof_requires_cleanup_and_exact_retained_report(self):
        p = base()
        with mock.patch.object(rt.harness, "_paths", return_value=(Path("release"), self.root)), \
                mock.patch.object(rt.harness, "_read_json", return_value={"ok": True}), \
                mock.patch.object(rt, "recertify_outbound"), \
                mock.patch.object(rt.harness, "verify_leg", return_value={"strict": True}):
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "proof differs"):
                rt.complete_check(p, self.root, {"strict": False})
        with mock.patch.object(rt.harness, "_read_json", return_value={"ok": False}), \
                mock.patch.object(rt.harness, "verify_leg") as verify:
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "cleanup"):
                rt.complete_check(p, self.root, {})
            verify.assert_not_called()

    def test_initial_changed_heads_actor_world_hash_release_before_input(self):
        for failure in ("revision", "actor", "world", "hash", "raw"):
            with self.subTest(failure=failure), ExitStack() as stack:
                p = base(); actors = {n: {"revision": 6} for n in ("a", "b")}
                stack.enter_context(mock.patch.object(rt.harness, "check_c_space"))
                stack.enter_context(mock.patch.object(rt.harness, "_mgba_pid_for", return_value=None))
                stack.enter_context(mock.patch.object(rt.harness, "_health_url", return_value=(None, SimpleNamespace(hostname="127.0.0.1", scheme="http", netloc="127.0.0.1:8080"))))
                stack.enter_context(mock.patch.dict(os.environ, {"COOP_HARNESS_USERNAME_A": "a", "COOP_HARNESS_USERNAME_B": "b", "COOP_HARNESS_PASSWORD": "private"}))
                lease = {"current_revision": 9 if failure == "revision" else 6, "session_id": "lease", "session_epoch": 1, "client_instance_id": "client"}
                acquire = stack.enter_context(mock.patch.object(rt.groups, "_acquire_after_release", return_value=lease))
                def call(server, method, path, **kwargs):
                    if path == "/v1/auth/login": return 200, json.dumps({"character_id": "wrong" if failure == "actor" else "a", "access_token": "private"}).encode()
                    if path == "/v1/sessions/release": return 200, b'{}'
                    if path.endswith("/snapshots"):
                        return 200, json.dumps({"snapshots": [{"revision": 6, "snapshot_id": "snapshot", "rom_world_id": 2 if failure == "world" else 1,
                            "files": [{"artifact": "character.sav", "sha256": "wrong" if failure == "hash" else p["players"][0]["source_sha256"]}]}]}).encode()
                    self.assertNotIn("/groups/", path)
                    return 200, b"wrong" if failure == "raw" else b"save"
                calls = stack.enter_context(mock.patch.object(rt.groups, "_call", side_effect=call))
                with self.assertRaises(rt.harness.HarnessFailure):
                    rt.initial_heads(p, actors)
                if failure == "actor": acquire.assert_not_called()
                else: self.assertEqual(sum(c.args[2] == "/v1/sessions/release" for c in calls.call_args_list), 1)

    def preload_setup(self):
        p = base(); p["release_id"] = "signed-family"
        source = self.root / "source"; source.mkdir()
        for relative in ("runtime/game.gba", ".complete", ".signed-release", "account.json", ".accepted-generation-secret"):
            file = source / relative; file.parent.mkdir(parents=True, exist_ok=True); file.write_bytes(relative.encode())
        for player in p["players"]: player["profile_localappdata"] = str(self.root / player["name"])
        envelope = {"payload": base64.b64encode(json.dumps({"artifacts": [{"id": "rom"}]}).encode()).decode()}
        stack = ExitStack(); self.addCleanup(stack.close)
        check = stack.enter_context(mock.patch.object(rt.harness, "check_installed_family"))
        stack.enter_context(mock.patch.object(rt.harness, "_paths", return_value=(self.root, self.root)))
        stack.enter_context(mock.patch.object(rt.harness, "_read_json", return_value=envelope))
        stack.enter_context(mock.patch.object(rt.harness, "check_c_space"))
        # Synthetic spare-volume files exercise exclusive linking only. The
        # accepted Windows canonicalization guard is mocked, never relaxed in
        # production (regular C profiles remain required for this live family).
        stack.enter_context(mock.patch.object(rt.harness, "require_plain_cache_path"))
        return p, source, check

    def test_preload_only_links_signed_bytes_and_idempotently_reuses(self):
        p, source, check = self.preload_setup()
        receipt = rt.preload_signed_cache(p, source)
        self.assertEqual(receipt["new_hardlinks"], 6)
        for target in receipt["targets"].values():
            folder = Path(target)
            self.assertTrue(os.path.samefile(source / "runtime/game.gba", folder / "runtime/game.gba"))
            self.assertFalse((folder / "account.json").exists())
            self.assertFalse((folder / ".accepted-generation-secret").exists())
            self.assertEqual({str(file.relative_to(folder)).replace("\\", "/") for file in folder.rglob("*") if file.is_file()},
                             {"runtime/game.gba", ".complete", ".signed-release"})
        reused = rt.preload_signed_cache(p, source)
        self.assertEqual(reused["new_hardlinks"], 0)
        self.assertEqual(check.call_count, 6)

    def test_preload_wrong_source_or_existing_bytes_has_zero_new_links(self):
        p, source, check = self.preload_setup()
        with mock.patch.object(rt.os, "link") as link:
            check.side_effect = rt.harness.HarnessFailure("wrong family")
            with self.assertRaises(rt.harness.HarnessFailure): rt.preload_signed_cache(p, source)
            link.assert_not_called()
            check.side_effect = None
            target = Path(p["players"][1]["profile_localappdata"]) / "Hoenn Sessions/runtime/releases/generations/signed-family/runtime/game.gba"
            target.parent.mkdir(parents=True); target.write_bytes(b"different")
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "changed existing"):
                rt.preload_signed_cache(p, source)
            link.assert_not_called()
            self.assertEqual(target.read_bytes(), b"different")

    def test_completed_reuse_reopens_outbound_baseline_journal_and_stage(self):
        real_complete = rt.complete_check
        m = self.lifecycle()
        completed = self.run_mock()
        root = Path(completed["plan_path"]).parent
        baseline = base(); baseline["run_dir"] = str(root / "journey")
        evidence = {"players": {}}
        files = {}
        for name in ("a", "b"):
            for label in ("baseline", "source", "staged", "journal"):
                file = root / (name + "-" + label)
                file.write_bytes((name + label).encode()); files[name, label] = file
            evidence["players"][name] = {label: str(files[name, label]) for label in ("source", "staged", "journal")}
            evidence["players"][name].update({label + "_sha256": rt.harness.digest(files[name, label]) for label in ("source", "staged", "journal")})
            player = next(p for p in baseline["players"] if p["name"] == name)
            player.update(source_save=str(files[name, "baseline"]), source_sha256=rt.harness.digest(files[name, "baseline"]))
        baseline_path = root / "outbound-evidence-plan.json"
        baseline_path.write_text(json.dumps(baseline))
        evidence_path = rt.capture_directory(Path(baseline["run_dir"]), "out", "evidence") / "evidence.json"
        evidence_path.parent.mkdir(parents=True); evidence_path.write_text(json.dumps(evidence))
        attested = copy.deepcopy(baseline)
        attested["departure_attestation"] = {"version": 1, "leg": "out", "verification": {"strict": True},
                                             "base_plan_sha256": rt.harness.digest(baseline_path), "evidence_sha256": rt.harness.digest(evidence_path)}
        Path(completed["plan_path"]).write_text(json.dumps(attested))
        complete = json.loads((root / "complete.json").read_text())
        complete["plan_sha256"] = rt.harness.digest(Path(completed["plan_path"]))
        (root / "complete.json").write_text(json.dumps(complete))
        for label in ("outbound-cleanup", "return-cleanup"):
            (root / (label + ".json")).write_text('{"ok":true}')
        returned_path = rt.capture_directory(Path(baseline["run_dir"]), "back", "evidence") / "evidence.json"
        returned_path.parent.mkdir(parents=True); returned_path.write_text('{}')
        def derive(original, leg, retained):
            self.assertEqual(leg, "out")
            # Existing departure-oracle tests exercise normalization semantics;
            # here real file hashes prove completed reuse wires every artifact
            # back through that oracle rather than trusting prior summary text.
            for player in original["players"]:
                rt.harness.require_hash(Path(player["source_save"]), player["source_sha256"], "baseline")
                item = retained["players"][player["name"]]
                for label in ("source", "staged", "journal"):
                    rt.harness.require_hash(Path(item[label]), item[label + "_sha256"], label)
            result = copy.deepcopy(attested)
            result["departure_attestation"].pop("base_plan_sha256")
            result["departure_attestation"].pop("evidence_sha256")
            return result
        m["complete"].side_effect = real_complete
        m["space"].reset_mock(); m["space"].side_effect = rt.harness.HarnessFailure("below 2 GiB")
        with mock.patch.object(rt.departure, "derive_plan", side_effect=derive) as recertify, \
                mock.patch.object(rt.harness, "verify_leg", return_value={"strict": True}) as verify, \
                mock.patch.object(rt.groups, "_call") as api:
            self.assertTrue(self.run_mock()["reused"])
            self.assertEqual(recertify.call_count, 1)
            verify.assert_called_once_with(attested, "back", {}, require_live_space=False)
            for label in ("baseline", "source", "journal", "staged"):
                for missing in (False, True):
                    with self.subTest(label=label, missing=missing):
                        file = files["a", label]; original = file.read_bytes()
                        if missing: file.unlink()
                        else: file.write_bytes(b"changed")
                        with self.assertRaises(rt.harness.HarnessFailure): self.run_mock()
                        file.write_bytes(original)
            verify.return_value = {"strict": False}
            with self.assertRaisesRegex(rt.harness.HarnessFailure, "return proof differs"):
                self.run_mock()
            api.assert_not_called()
        m["space"].assert_not_called()
        for label in ("author", "seed_plan", "prepare", "outbound", "return"):
            self.assertEqual(m[label].call_count, 1)
        m["launch"].assert_not_called()


class OutboundPairingTests(unittest.TestCase):
    """Outbound forms its group via b's desktop and proves it before and after the crossing."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir="S:/cormoria-build" if Path("S:/cormoria-build").is_dir() else None)
        self.addCleanup(self.temp.cleanup); self.root = Path(self.temp.name)

    def outbound_mocks(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        order = []
        def record(label, value=None):
            def effect(*args, **kwargs):
                order.append(label)
                return value() if callable(value) else value
            return effect
        p = base(); p["legs"][0].update(destination_world_id=2)
        m = {}
        for target, name, value in (
                (rt, "initial_heads", {"a": {}, "b": {}}),
                (rt, "presence_regions", lambda: ["REGION"]),
                (rt.groups, "require_ungrouped", {"a": {}, "b": {}}),
                (rt.groups, "create_pairing_code", {"code": "ABC-234", "code_sha256": "c" * 64,
                                                    "expires_at_unix_ms": 1, "inviter_character_id": "a"}),
                (rt.harness, "launch", {"a": 10, "b": 11}),
                (rt.harness, "start_games", {"a": 20, "b": 21}),
                (rt, "bind_games", {}),
                (rt.selected, "bind_loaded_sources", {}),
                (rt.groups, "pair_desktops", {"players": {}, "checked_at_unix_ms": 1}),
                (rt.harness, "drive", {}),
                (rt.harness, "wait_arrival_games", {"a": 30, "b": 31}),
                (rt.harness, "continue_arrivals", None),
                (rt.groups, "journal_group_id", "group"),
                (rt.groups, "partner_proof", {"players": {}, "checked_at_unix_ms": 2}),
                (rt.groups, "group_record", {"group_id": "group"}),
                (rt.harness, "stop_runtime", None),
                (rt.harness, "_paths", (Path("release"), self.root)),
                (rt.groups, "collect", {"group": "group.json"}),
                (rt.harness, "discover_evidence", {}),
                (rt.departure, "publish_plan", self.root / "attested.json"),
                (rt.harness, "close_desktops", None)):
            m[name] = stack.enter_context(mock.patch.object(target, name, side_effect=record(name, value)))
        stack.enter_context(mock.patch.object(rt.harness, "digest", return_value="d" * 64))
        stack.enter_context(mock.patch.object(rt.harness.psutil, "pid_exists", return_value=False))
        return p, m, order

    def test_code_before_launch_join_before_drive_and_post_arrival_proof_before_stop(self):
        p, m, order = self.outbound_mocks()
        rt.outbound(p, {}, self.root)
        position = {name: order.index(name) for name in order[::-1]}
        self.assertLess(position["require_ungrouped"], position["create_pairing_code"])
        self.assertLess(position["create_pairing_code"], position["launch"])
        self.assertLess(position["start_games"], position["pair_desktops"])
        self.assertLess(position["pair_desktops"], position["drive"])
        self.assertLess(position["continue_arrivals"], position["partner_proof"])
        self.assertLess(position["partner_proof"], order.index("stop_runtime"))
        self.assertLess(order.index("stop_runtime"), position["collect"])
        self.assertEqual(m["collect"].call_args.args[2], {"group_id": "group"})
        self.assertEqual(m["group_record"].call_args.args[2:], ("group", {"players": {}, "checked_at_unix_ms": 1},
                                                               {"players": {}, "checked_at_unix_ms": 2}))
        receipt = json.loads((self.root / "outbound-pairing.json").read_text())
        self.assertNotIn("code", receipt)

    def test_failed_join_or_lost_group_never_drives_or_collects(self):
        for boundary in ("pair_desktops", "partner_proof", "journal_group_id"):
            with self.subTest(boundary=boundary), tempfile.TemporaryDirectory(dir=self.root) as folder:
                self.root_backup, self.root = self.root, Path(folder)
                try:
                    p, m, order = self.outbound_mocks()
                    m[boundary].side_effect = rt.harness.HarnessFailure(boundary + " rejected")
                    with self.assertRaisesRegex(rt.harness.HarnessFailure, "rejected"):
                        rt.outbound(p, {}, self.root)
                    m["collect"].assert_not_called()
                    if boundary == "pair_desktops": m["drive"].assert_not_called()
                    self.assertIn("close_desktops", order)
                    self.assertFalse(json.loads((self.root / "outbound-cleanup.json").read_text())["alive_pids"])
                finally:
                    self.root = self.root_backup

    def test_occupied_group_or_code_failure_never_launches(self):
        for boundary in ("require_ungrouped", "create_pairing_code"):
            with self.subTest(boundary=boundary):
                p, m, order = self.outbound_mocks()
                m[boundary].side_effect = rt.harness.HarnessFailure("refused")
                with self.assertRaises(rt.harness.HarnessFailure):
                    rt.outbound(p, {}, self.root)
                m["launch"].assert_not_called()

    def test_factory_requires_desktop_pairing_group_mode(self):
        for mode in ("api-preformed", None):
            with self.subTest(mode=mode), mock.patch.object(rt.harness, "_read_json", return_value=base()):
                with self.assertRaisesRegex(rt.harness.HarnessFailure, "unsupported"):
                    rt.inputs(Path("plan"), Path("config"), {"adapter": "hoenn-debug-v1", "source_world_id": 1,
                                                             "group_mode": mode}, False)


def uuid_value(value):
    import uuid
    return uuid.UUID(int=value)


if __name__ == "__main__": unittest.main()
