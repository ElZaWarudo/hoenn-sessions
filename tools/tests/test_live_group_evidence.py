"""Collector admission and token-only group proofs; mocked server, no runtime actions."""
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/coop"))
import live_group_evidence as evidence

PROOF = {"group_id": "00000000-0000-0000-0000-000000000009", "members": [{"character_id": "ca"}, {"character_id": "cb"}],
         "world_zone": {"region": "CORMORIA"}, "pre_crossing": {}, "post_arrival": {"players": {}}}


class EvidenceContainmentTests(unittest.TestCase):
    def test_empty_drive_platform_keeps_containment_without_system_drive_false_positive(self):
        with mock.patch.object(evidence, "_capture_path", side_effect=[PurePosixPath("/spare/run/evidence"),
                                                                      PurePosixPath("/spare/run"), PurePosixPath("/C")]):
            self.assertEqual(evidence._evidence_output(Path("inside"), Path("run")), PurePosixPath("/spare/run/evidence"))
        with mock.patch.object(evidence, "_capture_path", side_effect=[PurePosixPath("/spare/outside/evidence"),
                                                                      PurePosixPath("/spare/run"), PurePosixPath("/C")]):
            with self.assertRaises(evidence.HarnessFailure):
                evidence._evidence_output(Path("outside"), Path("run"))

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_unc_namespace_aliases_match_without_network_io(self):
        run = Path(r"\\server\share\run")
        child = Path(r"\\?\UNC\server\share\run\evidence")
        with mock.patch.object(Path, "resolve", lambda path: path):
            self.assertEqual(evidence._evidence_output(child, run), child)

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_resolved_symlink_escape_is_rejected(self):
        run = Path(r"S:\run")
        child = run / "linked" / "evidence"
        with mock.patch.object(Path, "resolve", lambda path: Path(r"S:\outside\evidence") if path == child else path):
            with self.assertRaises(evidence.HarnessFailure):
                evidence._evidence_output(child, run)

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_mixed_namespaces_accept_inside_preserving_extended_io(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build") as folder:
            run = Path(folder).resolve()
            child = run / "captured-saves" / "actor" / "evidence"
            extended = Path("\\\\?\\" + str(child))
            self.assertEqual(evidence._evidence_output(extended, run), extended)
            self.assertEqual(evidence._evidence_output(child, Path("\\\\?\\" + str(run))), extended)
            self.assertFalse(child.exists())

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_outside_and_c_output_rejected_before_api_or_directory_creation(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build") as folder:
            run = Path(folder).resolve()
            for output, root in ((run.parent / (run.name + "-outside") / "evidence", run),
                                 (Path(r"\\?\C:\cormoria-no-write\evidence"), Path(r"C:\cormoria-no-write")),
                                 (Path(r"C:\cormoria-no-write\evidence"), Path(r"\\?\C:\cormoria-no-write"))):
                with self.subTest(output=output), \
                        mock.patch.object(evidence, "_leg", return_value={}), \
                        mock.patch.object(evidence, "_paths", return_value=(run / "release", root)), \
                        mock.patch.object(evidence, "_call") as api, \
                        mock.patch.object(evidence, "_health_url") as health:
                    with self.assertRaisesRegex(evidence.HarnessFailure, "spare-volume"):
                        evidence.collect({}, "leg", PROOF, output)
                    api.assert_not_called()
                    health.assert_not_called()
                    self.assertFalse(output.exists())

    @unittest.skipUnless(os.name == "nt", "native Windows namespace containment")
    def test_valid_mixed_namespace_collector_reaches_next_gate_without_api(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build") as folder:
            run = Path(folder).resolve()
            output = Path("\\\\?\\" + str(run / "captured-saves" / "evidence"))
            with mock.patch.object(evidence, "_leg", return_value={}), \
                    mock.patch.object(evidence, "_paths", return_value=(run / "release", run)), \
                    mock.patch.object(evidence, "_call") as api, \
                    mock.patch.object(evidence, "_health_url", side_effect=RuntimeError("next gate reached")):
                with self.assertRaisesRegex(RuntimeError, "next gate reached"):
                    evidence.collect({}, "leg", PROOF, output)
                api.assert_not_called()
                self.assertFalse(output.exists())


def plan():
    return {"server_url": "http://127.0.0.1:8080", "release_dir": "release",
            "players": [{"name": "a", "character_id": "ca", "profile_localappdata": "profile-a"},
                        {"name": "b", "character_id": "cb", "profile_localappdata": "profile-b"}],
            "legs": [{"name": "out", "source_world_id": 1, "destination_world_id": 2}]}


ACCOUNTS = {"COOP_HARNESS_USERNAME_A": "HarnessA", "COOP_HARNESS_USERNAME_B": "HarnessB",
            "COOP_HARNESS_PASSWORD": "private"}


def partner(username, *, active=True, online=True, region="CORMORIA", live=True):
    zone = {"region": region, "map": "HARBOR", "instance": 1}
    value = {"username": username, "online": online, "last_seen_at": None,
             "world_zone": {"region": "HOENN", "map": "FERRY", "instance": 1} if live else zone,
             "badge_count": 0, "group_active": active}
    if live:
        value["live_world_zone"] = zone
    return value


class FakeServer:
    """Token-only partner reads plus fenced pairing-code issuance."""

    def __init__(self, partners, *, code="ABC-234", acquire_status=200, issue_status=201, link=None):
        self.partners, self.code, self.calls, self.active = partners, code, [], set()
        self.acquire_status, self.issue_status, self.link = acquire_status, issue_status, link

    def __call__(self, server, method, path, *, payload=None, token=None, lease=None, raw=None):
        self.calls.append((method, path, token, lease))
        if path == "/v1/auth/login":
            name = "a" if payload["username"] == "HarnessA" else "b"
            return 200, json.dumps({"character_id": "c" + name, "access_token": "token-" + name}).encode()
        if path == "/v1/group/partner":
            assert lease is None, "partner status must be token-only"
            value = self.partners[token[-1]]
            return 200, json.dumps({"api_version": 1, "partner": value}).encode()
        if path == "/v1/sessions/acquire-world":
            if self.acquire_status != 200:
                return self.acquire_status, b'{"error":{"code":"conflict"}}'
            self.active.add(payload["character_id"])
            return 200, json.dumps({"lease": {"session_id": "s", "current_revision": 6, "session_epoch": 1,
                                              "client_instance_id": payload["client_instance_id"]}}).encode()
        if path == "/v1/groups/pairing-codes":
            assert lease is not None and set(payload) == {"api_version", "character_id", "session_id",
                                                          "current_revision", "session_epoch", "client_instance_id"}
            link = self.link if self.link is not None else "hoenn-sessions://join/" + str(self.code)
            return self.issue_status, json.dumps({"api_version": 1, "code": self.code, "join_link": link,
                                                  "expires_at": 1_791_000_000_000}).encode()
        if path == "/v1/sessions/release":
            self.active.discard(payload["character_id"])
            return 200, b"{}"
        raise AssertionError("unexpected request " + path)


class GroupProofTests(unittest.TestCase):
    def setUp(self):
        stack = ExitStack(); self.addCleanup(stack.close)
        stack.enter_context(mock.patch.dict(os.environ, ACCOUNTS))
        stack.enter_context(mock.patch.object(evidence, "checkpoint"))
        stack.enter_context(mock.patch.object(evidence, "_paths", return_value=(Path("release"), Path("run"))))
        self.stack = stack

    def server(self, partners, **kwargs):
        fake = FakeServer(partners, **kwargs)
        self.stack.enter_context(mock.patch.object(evidence, "_call", side_effect=fake))
        return fake

    def test_active_group_with_expected_partner_in_region_is_proved_token_only(self):
        fake = self.server({"a": partner("HarnessB"), "b": partner("HarnessA")})
        proof = evidence.partner_proof(plan(), ["CORMORIA"], "out post-arrival")
        self.assertEqual({n: p["partner_character_id"] for n, p in proof["players"].items()}, {"a": "cb", "b": "ca"})
        self.assertEqual({p["partner_region"] for p in proof["players"].values()}, {"CORMORIA"})
        self.assertEqual({p["zone_source"] for p in proof["players"].values()}, {"live"})
        self.assertTrue(all(lease is None for method, path, _, lease in fake.calls))
        self.assertFalse(any("/v1/groups/" in path for _, path, _, _ in fake.calls))

    def test_closed_wrong_partner_offline_missing_or_wrong_region_fail_closed(self):
        cases = {"closed": partner("HarnessA", active=False), "wrong-partner": partner("Intruder"),
                 "offline": partner("HarnessA", online=False), "missing": None,
                 "wrong-region": partner("HarnessA", region="HOENN")}
        for label, b_view in cases.items():
            with self.subTest(label=label), mock.patch.object(evidence, "_call",
                                                              side_effect=FakeServer({"a": partner("HarnessB"), "b": b_view})):
                with self.assertRaises(evidence.HarnessFailure):
                    evidence.partner_proof(plan(), ["CORMORIA"], "out post-arrival")

    def test_saved_zone_is_used_only_without_live_presence(self):
        self.server({"a": partner("HarnessB", live=False), "b": partner("HarnessA", live=False)})
        proof = evidence.partner_proof(plan(), ["CORMORIA"], "pre")
        self.assertEqual({p["zone_source"] for p in proof["players"].values()}, {"saved"})

    def test_join_confirmation_waits_for_active_and_rejects_wrong_partner(self):
        with mock.patch.object(evidence, "_call", side_effect=FakeServer({"a": None, "b": None})):
            self.assertFalse(evidence.partner_active(plan()))
        with mock.patch.object(evidence, "_call", side_effect=FakeServer({"a": partner("HarnessB", active=False),
                                                                         "b": partner("HarnessA")})):
            self.assertFalse(evidence.partner_active(plan()))
        with mock.patch.object(evidence, "_call", side_effect=FakeServer({"a": partner("HarnessB"), "b": partner("HarnessA")})):
            self.assertTrue(evidence.partner_active(plan()))
        with mock.patch.object(evidence, "_call", side_effect=FakeServer({"a": partner("Intruder"), "b": partner("HarnessA")})):
            with self.assertRaisesRegex(evidence.HarnessFailure, "unexpected partner"):
                evidence.partner_active(plan())

    def test_server_lowercased_usernames_match_mixed_case_environment_accounts(self):
        # The server stores usernames lowercased; the environment keeps the registered spelling.
        self.server({"a": partner("harnessb"), "b": partner("harnessa")})
        self.assertTrue(evidence.partner_active(plan()))
        proof = evidence.partner_proof(plan(), ["CORMORIA"], "out post-arrival")
        self.assertEqual({p["partner_username"] for p in proof["players"].values()}, {"harnessa", "harnessb"})
        with mock.patch.object(evidence, "_call", side_effect=FakeServer({"a": partner("harnessa"),
                                                                         "b": partner("harnessa")})):
            with self.assertRaisesRegex(evidence.HarnessFailure, "unexpected partner"):
                evidence.partner_active(plan())
        with mock.patch.dict(os.environ, {"COOP_HARNESS_USERNAME_B": "HARNESSA"}):
            with self.assertRaisesRegex(evidence.HarnessFailure, "two distinct"):
                evidence.partner_active(plan())

    def test_pairing_requires_no_active_group(self):
        self.server({"a": partner("HarnessB", active=False), "b": None})
        self.assertEqual(evidence.require_ungrouped(plan(), "back")["a"]["previous_partner"], "HarnessB")
        with mock.patch.object(evidence, "_call", side_effect=FakeServer({"a": partner("HarnessB"), "b": None})):
            with self.assertRaisesRegex(evidence.HarnessFailure, "already exists"):
                evidence.require_ungrouped(plan(), "back")

    def test_pairing_code_is_issued_under_a_released_fenced_lease(self):
        fake = self.server({})
        code = evidence.create_pairing_code(plan(), "out")
        self.assertEqual(code["code"], "ABC-234"); self.assertEqual(code["inviter_character_id"], "ca")
        self.assertFalse(fake.active)
        self.assertEqual([p for _, p, _, _ in fake.calls].count("/v1/sessions/release"), 1)

    def test_pairing_code_failures_release_and_never_return_a_code(self):
        for kwargs in ({"issue_status": 409}, {"code": "abc-234"}, {"code": "ABC-1I0"}, {"link": "https://elsewhere/ABC-234"}):
            with self.subTest(kwargs=kwargs):
                fake = FakeServer({}, **kwargs)
                with mock.patch.object(evidence, "_call", side_effect=fake):
                    with self.assertRaises(evidence.HarnessFailure):
                        evidence.create_pairing_code(plan(), "out")
                self.assertFalse(fake.active)
        fake = FakeServer({}, acquire_status=409)
        with mock.patch.object(evidence, "_call", side_effect=fake):
            with self.assertRaises(evidence.HarnessFailure):
                evidence.create_pairing_code(plan(), "out")
        self.assertFalse(any(path == "/v1/groups/pairing-codes" for _, path, _, _ in fake.calls))

    def proofs(self):
        def make(t, region):
            return {"checked_at_unix_ms": t, "players": {
                "a": {"character_id": "ca", "partner_character_id": "cb", "group_active": True, "partner_online": True, "partner_region": region},
                "b": {"character_id": "cb", "partner_character_id": "ca", "group_active": True, "partner_online": True, "partner_region": region}}}
        return make(1, "HOENN"), make(2, "CORMORIA")

    def test_group_record_binds_journal_id_and_both_live_proofs(self):
        before, after = self.proofs()
        record = evidence.group_record(plan(), plan()["legs"][0], "00000000-0000-0000-0000-000000000009", before, after)
        self.assertEqual(record["world_zone"], {"region": "CORMORIA"})
        self.assertEqual(evidence._sanitize_group_evidence(record)["members"], [{"character_id": "ca"}, {"character_id": "cb"}])
        for change in ("inactive", "partner", "order", "split"):
            with self.subTest(change=change):
                before, after = self.proofs()
                if change == "inactive": after["players"]["b"]["group_active"] = False
                if change == "partner": before["players"]["a"]["partner_character_id"] = "other"
                if change == "order": before["checked_at_unix_ms"] = 3
                if change == "split": after["players"]["a"]["partner_region"] = "HOENN"
                with self.assertRaises(evidence.HarnessFailure):
                    evidence.group_record(plan(), plan()["legs"][0], "00000000-0000-0000-0000-000000000009", before, after)

    def test_crossing_journals_must_name_one_group(self):
        journal = lambda gid: [(1, Path("j"), {"intent": {"request": {"group_id": gid}}})]
        same = "00000000-0000-0000-0000-000000000009"
        with mock.patch.object(evidence, "_journal_candidates", side_effect=[journal(same), journal(same)]):
            self.assertEqual(evidence.journal_group_id(plan(), plan()["legs"][0]), same)
        for pair in ((journal(same), journal("00000000-0000-0000-0000-000000000008")), (journal(same), [])):
            with mock.patch.object(evidence, "_journal_candidates", side_effect=list(pair)):
                with self.assertRaises(evidence.HarnessFailure):
                    evidence.journal_group_id(plan(), plan()["legs"][0])

    def test_collect_never_reads_a_group_after_stop(self):
        with tempfile.TemporaryDirectory(dir="S:/cormoria-build" if Path("S:/cormoria-build").is_dir() else None) as folder:
            run = Path(folder).resolve()
            calls = []
            def call(server, method, path, **kwargs):
                calls.append(path)
                if path == "/v1/auth/login":
                    return 200, json.dumps({"access_token": "t"}).encode()
                if path.endswith("/snapshots"):
                    return 200, json.dumps({"snapshots": [{"revision": 7, "rom_world_id": 2, "files": [
                        {"artifact": "character.sav", "sha256": hashlib.sha256(b"save").hexdigest()}]}]}).encode()
                if "resume-package" in path:
                    return 200, b"save"
                return 200, b"{}"
            lease = {"session_id": "s", "current_revision": 7, "session_epoch": 1, "client_instance_id": "c"}
            with mock.patch.object(evidence, "_paths", return_value=(run, run)), \
                    mock.patch.object(evidence, "_evidence_output", side_effect=lambda output, _run: output), \
                    mock.patch.object(evidence, "_read_json", return_value={"worlds": [{"world_id": 2, "presence_regions": ["CORMORIA"]}]}), \
                    mock.patch.object(evidence, "_acquire_after_release", return_value=lease), \
                    mock.patch.object(evidence, "_call", side_effect=call):
                result = evidence.collect(plan(), "out", PROOF, run / "server-evidence")
                self.assertFalse(any("/v1/groups" in path or "/v1/group/" in path for path in calls))
                self.assertEqual(json.loads(Path(result["group"]).read_text())["group_id"], PROOF["group_id"])
                with self.assertRaisesRegex(evidence.HarnessFailure, "post-Stop group reads were removed"):
                    evidence.collect(plan(), "out", PROOF["group_id"], run / "server-evidence")
                wrong = dict(PROOF, world_zone={"region": "HOENN"})
                with self.assertRaisesRegex(evidence.HarnessFailure, "destination region"):
                    evidence.collect(plan(), "out", wrong, run / "server-evidence")


if __name__ == "__main__":
    unittest.main()
