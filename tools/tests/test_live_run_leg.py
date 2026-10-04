"""Mocked chosen-leg lifecycle, fresh per-session group proofs and leased read guards; no server or UI."""
from contextlib import ExitStack
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_run_leg as runner


def plan():
    inputs = []
    for name in ("a", "b"):
        inputs += [{"player": name, "key": key, **({"expect_presence_published": True} if key == "gba_a" else {})}
                   for key in ("gba_b", "gba_start", "gba_a")]
    previous = {"name": "out", "source_world_id": 1, "destination_world_id": 2, "portal_id": "out"}
    leg = {"name": "back", "source_world_id": 2, "destination_world_id": 1, "portal_id": "back",
           "destination_base_leg": "out", "inputs": [*copy.deepcopy(inputs), {"player": "b", "key": "gba_a", "expect_travel": True}],
           "arrival_inputs": inputs}
    return {"players": [{"name": name, "character_id": name, "profile_localappdata": "profile-" + name} for name in ("a", "b")],
            "legs": [previous, leg]}


OUTBOUND = "00000000-0000-0000-0000-00000000000a"
FRESH = "00000000-0000-0000-0000-00000000000b"


def proof():
    return {"players": {name: {"revision": 8, "snapshot_id": name + "snapshot", "sha256": hashlib.sha256(b"save").hexdigest()} for name in ("a", "b")},
            "group": {"group_id": OUTBOUND, "members": [{"character_id": "a"}, {"character_id": "b"}], "world_zone": {"region": "source"}},
            "presence_regions": ["source"]}


class RunLegTests(unittest.TestCase):
    def lifecycle(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        m = {}
        for name, value in {"preflight": {}, "_paths": (Path("release"), Path("run")),
                            "launch": {"a": 10, "b": 11},
                            "wait_arrival_games": {"a": 30, "b": 31}, "drive": {}, "continue_arrivals": None,
                            "close_desktops": None, "checkpoint": None,
                            "discover_evidence": {"retained": True}, "verify_leg": {"passed": True},
                            }.items():
            m[name] = stack.enter_context(mock.patch.object(runner.harness, name, return_value=value))
        for name, value in {"start_games": {"a": 20, "b": 21}, "stop_runtime": None}.items():
            m[name] = stack.enter_context(mock.patch.object(runner.controls, name, return_value=value))
        for name, value in {"preceding_proof": proof(), "guard_heads": None, "bind_games": {}, "bind_loaded_sources": {}}.items():
            m[name] = stack.enter_context(mock.patch.object(runner, name, return_value=value))
        m["collect"] = stack.enter_context(mock.patch.object(runner.groups, "collect", return_value={"group": "group.json"}))
        for name, value in {"require_ungrouped": {"a": {}, "b": {}},
                            "create_pairing_code": {"code": "ABC-234", "code_sha256": "c" * 64,
                                                    "expires_at_unix_ms": 1, "inviter_character_id": "a"},
                            "pair_desktops": {"players": {}, "checked_at_unix_ms": 1},
                            "journal_group_id": FRESH, "partner_proof": {"players": {}, "checked_at_unix_ms": 2},
                            "group_record": {"group_id": FRESH, "world_zone": {"region": "HOENN"}}}.items():
            m[name] = stack.enter_context(mock.patch.object(runner.groups, name, return_value=value))
        m["presence_regions"] = stack.enter_context(mock.patch.object(runner, "presence_regions", return_value=["region"]))
        return m

    def test_only_selected_leg_executes_with_both_legs_preserved(self):
        p = plan(); original = copy.deepcopy(p); m = self.lifecycle()
        self.assertEqual(runner.run_leg(p, "back"), {"passed": True})
        self.assertEqual(p, original)
        m["drive"].assert_called_once_with(p, "back", {"a": 20, "b": 21})
        verification_plan = m["verify_leg"].call_args.args[0]
        self.assertEqual([l["name"] for l in verification_plan["legs"]], ["out", "back"])
        self.assertEqual(m["stop_runtime"].call_count, 2)
        m["close_desktops"].assert_called_once()

    def test_return_forms_fresh_group_before_drive_and_proves_it_after_arrival_before_stop(self):
        p = plan(); m = self.lifecycle()
        manager = mock.Mock()
        for name in ("require_ungrouped", "create_pairing_code", "launch", "pair_desktops", "drive",
                     "continue_arrivals", "partner_proof", "stop_runtime", "collect"):
            manager.attach_mock(m[name], name)
        runner.run_leg(p, "back")
        order = [c[0] for c in manager.mock_calls]
        first = {name: order.index(name) for name in set(order)}
        self.assertLess(first["require_ungrouped"], first["create_pairing_code"])
        self.assertLess(first["create_pairing_code"], first["launch"])
        self.assertLess(first["pair_desktops"], first["drive"])
        self.assertLess(first["continue_arrivals"], first["partner_proof"])
        self.assertLess(first["partner_proof"], first["stop_runtime"])
        self.assertLess(first["stop_runtime"], first["collect"])
        self.assertEqual(m["group_record"].call_args.args[2], FRESH)
        self.assertEqual(m["collect"].call_args.args[2], {"group_id": FRESH, "world_zone": {"region": "HOENN"}})

    def test_return_reusing_outbound_group_fails_without_evidence(self):
        m = self.lifecycle(); m["journal_group_id"].return_value = OUTBOUND
        with self.assertRaisesRegex(runner.harness.HarnessFailure, "fresh group is required"):
            runner.run_leg(plan(), "back")
        m["partner_proof"].assert_not_called(); m["collect"].assert_not_called()
        m["close_desktops"].assert_called_once()

    def test_closed_or_wrong_partner_group_fails_at_each_proof(self):
        for boundary in ("pair_desktops", "partner_proof", "require_ungrouped", "create_pairing_code"):
            with self.subTest(boundary=boundary):
                m = self.lifecycle()
                m[boundary].side_effect = runner.harness.HarnessFailure("group is not Active")
                with self.assertRaisesRegex(runner.harness.HarnessFailure, "not Active"):
                    runner.run_leg(plan(), "back")
                m["collect"].assert_not_called()
                if boundary in ("require_ungrouped", "create_pairing_code"):
                    m["launch"].assert_not_called()
                if boundary == "pair_desktops":
                    m["drive"].assert_not_called(); m["close_desktops"].assert_called_once()

    def test_unsafe_input_rejected_before_launch(self):
        for value in ({"key": "unknown"}, {"hold_ms": True}, {"wait_ms": 30001}, {"timeout_ms": 120001}, {"evil": True}):
            p = plan(); p["legs"][1]["inputs"][0].update(value); m = self.lifecycle()
            with self.subTest(value=value), self.assertRaises(runner.harness.HarnessFailure):
                runner.run_leg(p, "back")
            m["launch"].assert_not_called()

    def test_bad_label_and_arrival_departure_rejected(self):
        p = plan(); p["legs"][1]["name"] = "bad/name"
        with self.assertRaises(runner.harness.HarnessFailure):
            runner.validate_inputs(p, "bad/name")
        p = plan(); p["legs"][1]["arrival_inputs"][0]["expect_travel"] = True
        with self.assertRaises(runner.harness.HarnessFailure):
            runner.validate_inputs(p, "back")

    def test_proof_or_head_failure_never_launches(self):
        for boundary in ("preceding_proof", "guard_heads"):
            m = self.lifecycle(); m[boundary].side_effect = RuntimeError("rejected")
            with self.subTest(boundary=boundary), self.assertRaisesRegex(RuntimeError, "rejected"):
                runner.run_leg(plan(), "back")
            m["launch"].assert_not_called()

    def test_loaded_source_failure_stops_before_input(self):
        m = self.lifecycle(); m["bind_loaded_sources"].side_effect = RuntimeError("source changed")
        with self.assertRaisesRegex(RuntimeError, "source changed"):
            runner.run_leg(plan(), "back")
        m["drive"].assert_not_called(); m["close_desktops"].assert_called_once()

    def test_original_failure_preserved_and_both_cleanup_attempted(self):
        m = self.lifecycle(); error = RuntimeError("input failure")
        m["drive"].side_effect = error
        m["stop_runtime"].side_effect = RuntimeError("stop failure")
        m["close_desktops"].side_effect = RuntimeError("close failure")
        with self.assertRaises(RuntimeError) as raised:
            runner.run_leg(plan(), "back")
        self.assertIs(raised.exception, error); self.assertEqual(len(error.__notes__), 2)
        m["wait_arrival_games"].assert_not_called()

    def head_mocks(self, changed=None, release_failure=False):
        stack = ExitStack(); self.addCleanup(stack.close)
        stack.enter_context(mock.patch.dict(os.environ, {"COOP_HARNESS_PASSWORD": "test", "COOP_HARNESS_USERNAME_A": "a", "COOP_HARNESS_USERNAME_B": "b"}))
        stack.enter_context(mock.patch.object(runner.harness, "_health_url", return_value=(None, SimpleNamespace(hostname="127.0.0.1", scheme="http", netloc="127.0.0.1:8080"))))
        stack.enter_context(mock.patch.object(runner.harness, "check_c_space"))
        lease = {"current_revision": 8, "session_id": "lease", "session_epoch": 1, "client_instance_id": "client"}
        if changed == "revision": lease["current_revision"] = 9
        acquire = stack.enter_context(mock.patch.object(runner.groups, "_acquire_after_release", return_value=lease))
        def call(server, method, path, **kwargs):
            if path == "/v1/auth/login":
                actor = kwargs["payload"]["username"]
                return 200, json.dumps({"character_id": "other" if changed == "actor" else actor, "access_token": "private"}).encode()
            if path == "/v1/sessions/release":
                if release_failure: return 500, b'{}'
                return 200, b'{}'
            if path.endswith("/snapshots"):
                actor = path.split("/")[3]
                return 200, json.dumps({"snapshots": [{"revision": 8, "snapshot_id": actor + "snapshot",
                    "rom_world_id": 1 if changed == "world" else 2,
                    "files": [{"artifact": "character.sav", "sha256": "bad" if changed == "hash" else hashlib.sha256(b"save").hexdigest()}]}]}).encode()
            if "resume-package" in path: return 200, b"wrong" if changed == "raw" else b"save"
            raise AssertionError("unexpected head-guard request " + path)
        calls = stack.enter_context(mock.patch.object(runner.groups, "_call", side_effect=call))
        return calls, acquire

    def test_head_guard_checks_actual_revision_manifest_raw_and_group_then_releases(self):
        calls, acquire = self.head_mocks()
        result = runner.guard_heads(plan(), plan()["legs"][1], proof())
        self.assertEqual(result["a"]["revision"], 8)
        self.assertEqual(set(result["a"]), {"character_id", "revision", "snapshot_id", "world_id", "sha256"})
        self.assertFalse(any("/groups/" in c.args[2] for c in calls.call_args_list))
        self.assertEqual(sum(c.args[2] == "/v1/sessions/release" for c in calls.call_args_list), 2)
        self.assertTrue(all(c.args[3] == 0 for c in acquire.call_args_list))

    def test_wrong_head_boundaries_release_and_stop_before_second_actor(self):
        for changed in ("revision", "world", "hash", "raw"):
            with self.subTest(changed=changed):
                calls, _ = self.head_mocks(changed)
                with self.assertRaises(runner.harness.HarnessFailure):
                    runner.guard_heads(plan(), plan()["legs"][1], proof())
                self.assertEqual(sum(c.args[2] == "/v1/sessions/release" for c in calls.call_args_list), 1)

    def test_wrong_login_actor_never_acquires(self):
        _, acquire = self.head_mocks("actor")
        with self.assertRaises(runner.harness.HarnessFailure):
            runner.guard_heads(plan(), plan()["legs"][1], proof())
        acquire.assert_not_called()

    def test_release_failure_blocks_launch_and_preserves_head_error(self):
        self.head_mocks("world", release_failure=True)
        with self.assertRaises(runner.harness.HarnessFailure) as raised:
            runner.guard_heads(plan(), plan()["legs"][1], proof())
        self.assertIn("snapshot/world", str(raised.exception))
        self.assertEqual(len(raised.exception.__notes__), 1)

    def test_loaded_save_path_and_desktop_ownership(self):
        p = plan(); profile = Path("profile-a").resolve()
        rom = profile / "Hoenn Sessions/sessions/coop-session-unit/world.gba"
        game = mock.Mock(); game.cmdline.return_value = [str(rom)]
        desktop = mock.Mock(); desktop.children.return_value = [SimpleNamespace(pid=20), SimpleNamespace(pid=21)]
        with mock.patch.object(runner.harness.psutil, "Process", side_effect=lambda pid: game if pid in (20, 21) else desktop), \
                mock.patch.object(runner.harness, "require_hash") as check:
            p["players"][1]["profile_localappdata"] = str(profile)
            result = runner.bind_loaded_sources(p, {"a": 20, "b": 21}, {"a": 10, "b": 11}, proof())
            self.assertEqual(check.call_count, 2); self.assertEqual(result["a"]["sha256"], proof()["players"]["a"]["sha256"])
            game.cmdline.return_value = [str(Path("outside.gba").resolve())]
            with self.assertRaises(runner.harness.HarnessFailure):
                runner.bind_loaded_sources(p, {"a": 20, "b": 21}, {"a": 10, "b": 11}, proof())
            desktop.children.return_value = []
            with self.assertRaises(runner.harness.HarnessFailure):
                runner.bind_loaded_sources(p, {"a": 20, "b": 21}, {"a": 10, "b": 11}, proof())

    def preceding_mocks(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        p = plan(); previous = p["legs"][0]
        report = {"fixture_key": "fixture", **{k: previous[k] for k in ("source_world_id", "destination_world_id", "portal_id")},
                  "players": [], "group": proof()["group"]}
        prior = {"players": {}}
        receipts, journals = [], {}
        for name in ("a", "b"):
            report["players"].append({"name": name, "character_id": name, "source_sha256": name + "source",
                                      "exact_journal_source_inspected": True, "destination_sha256": name + "stage"})
            prior["players"][name] = {"journal": name + "journal", "journal_sha256": "f" * 64,
                                      "journal_source_sha256": name + "source", "source": name + "source.sav", "staged": name + "stage.sav"}
            receipts.append({"player": name, "source_sha256": name + "source", "staged_sha256": name + "stage", "journal_sha256": "f" * 64})
            journals[name + "journal"] = {"character_id": name, "phase": "committed",
                "terminal": {"committed": {"own_world_id": 2, "own_revision": 8, "own_snapshot_id": name + "snapshot"}},
                "stage": {"destination_world_id": 2, "destination_save_sha256": name + "stage"},
                "intent": {"source_world_id": 1, "request": {"portal_id": "out"}}}
        p["departure_attestation"] = {"version": 1, "leg": "out", "verification": report, "players": receipts}
        events = [{"boundary": "out-verified", **copy.deepcopy(report)}]
        def read(path, label):
            if label == "selected checkpoints": return {"events": events}
            if label == "preceding evidence": return prior
            return {"worlds": [{"world_id": 2, "presence_regions": ["source"]}]}
        stack.enter_context(mock.patch.object(runner.harness, "_paths", return_value=(Path("release"), Path("run"))))
        stack.enter_context(mock.patch.object(runner.harness, "_read_json", side_effect=read))
        stack.enter_context(mock.patch.object(runner.harness, "_fixture_key", return_value="fixture"))
        stack.enter_context(mock.patch.object(runner.harness, "_read_evidence_journal", side_effect=lambda item, _: journals[item["journal"]]))
        stack.enter_context(mock.patch.object(runner.harness, "require_hash"))
        parked = stack.enter_context(mock.patch.object(runner.harness, "_destination_base"))
        candidates = stack.enter_context(mock.patch.object(runner.harness, "_journal_candidates", return_value=[]))
        exists = stack.enter_context(mock.patch.object(Path, "exists", return_value=False))
        return p, events, journals, parked, candidates, exists

    def test_preceding_proof_binds_parked_actors_and_explicit_server_revision(self):
        p, _, _, parked, _, _ = self.preceding_mocks()
        result = runner.preceding_proof(p, p["legs"][1])
        self.assertEqual(parked.call_count, 2)
        self.assertEqual(result["players"]["a"], {"revision": 8, "snapshot_id": "asnapshot", "sha256": "astage"})

    def test_replay_and_wrong_prior_proof_rejected(self):
        for boundary in ("verified", "journal", "evidence", "actor", "fixture", "commit"):
            with self.subTest(boundary=boundary):
                p, events, journals, _, candidates, exists = self.preceding_mocks()
                if boundary == "verified": events.append({"boundary": "back-verified"})
                if boundary == "journal": candidates.return_value = [mock.sentinel.committed]
                if boundary == "evidence": exists.return_value = True
                if boundary == "actor": journals["ajournal"]["character_id"] = "other"
                if boundary == "fixture": p["departure_attestation"]["verification"]["fixture_key"] = "other"
                if boundary == "commit": journals["ajournal"]["terminal"]["committed"]["own_world_id"] = 1
                with self.assertRaises(runner.harness.HarnessFailure):
                    runner.preceding_proof(p, p["legs"][1])


if __name__ == "__main__":
    unittest.main()
