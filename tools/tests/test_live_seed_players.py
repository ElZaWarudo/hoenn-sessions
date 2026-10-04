"""Test seed fences, reuse, upload origin and cleanup without live accounts."""

import json
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/coop"))
import live_seed_players as seed
from live_group_evidence import _call, _acquire_after_release
from live_harness_oracles import read_flash


class SeedBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.save = read_flash(ROOT / "tools/tests/fixtures/arrival-v3-main-lilycove.sav")
        self.plan = {"server_url": "http://127.0.0.1:18084", "seed_world_id": 1,
                     "players": [{"name": "a"}, {"name": "b"}]}

    def run_seed(self, *, revision=0, changed=False, upload_status=204,
                 release_status=200, off_origin=False):
        calls = []
        def api(server, method, path, **kwargs):
            calls.append((method, path))
            if path.endswith("/login"):
                return 200, json.dumps({"character_id": kwargs["payload"]["username"], "access_token": "dummy"}).encode()
            if path.endswith("/acquire"):
                return 200, json.dumps({"session_id": "session", "session_epoch": 1,
                                       "character_id": kwargs["payload"]["character_id"],
                                       "client_instance_id": "client", "current_revision": revision}).encode()
            if "resume-package" in path:
                return 200, b"changed" if changed else self.save.path.read_bytes()
            if path.endswith("/tickets") or path.endswith("/release"):
                return (release_status if path.endswith("/release") else 200), b"{}"
            if path.endswith("/prepare"):
                return 200, json.dumps({"snapshot_id": kwargs["payload"]["snapshot_id"],
                                       "idempotency_key": "prepare", "upload_targets": [
                                           {"artifact": "character.sav", "url": (
                                               "http://other.invalid/upload" if off_origin
                                               else server + "/upload")}] }).encode()
            if path == "/upload":
                return upload_status, b"{}"
            if path.endswith("/finalize"):
                return 200, json.dumps({"revision": kwargs["payload"]["revision"]}).encode()
            self.fail("unexpected API boundary " + path)
        validated = {"world": {"build": {}}, "saves": {"a": [self.save], "b": [self.save]}}
        with mock.patch.object(seed, "validate_lineages", return_value=validated), \
             mock.patch.object(seed, "check_c_space"), \
             mock.patch.object(seed, "_paths", return_value=(Path("release"), Path("run"))), \
             mock.patch.object(seed, "checkpoint"), \
             mock.patch.object(seed, "_call", side_effect=api), \
             mock.patch.dict(seed.os.environ, {"COOP_HARNESS_PASSWORD": "dummy",
                                                "COOP_HARNESS_USERNAME_A": "a",
                                                "COOP_HARNESS_USERNAME_B": "b"}):
            try:
                return calls, seed.seed(self.plan), None
            except Exception as error:
                return calls, None, error

    def test_complete_seed_reuses_head_without_any_snapshot_write(self):
        calls, result, error = self.run_seed(revision=1)
        self.assertIsNone(error)
        self.assertEqual(len(result["players"]), 2)
        self.assertFalse(any(p.endswith(("prepare", "finalize")) or m == "PUT" for m,p in calls))
        self.assertEqual(sum(p.endswith("release") for _,p in calls), 2)

    def test_changed_head_is_rejected_before_snapshot_write_and_released(self):
        calls, _, error = self.run_seed(revision=1, changed=True)
        self.assertIn("changed player head", str(error))
        self.assertFalse(any(p.endswith(("prepare", "finalize")) or m == "PUT" for m,p in calls))
        self.assertTrue(calls[-1][1].endswith("release"))

    def test_off_origin_target_causes_no_put(self):
        calls, _, error = self.run_seed(off_origin=True)
        self.assertIn("left pinned loopback", str(error))
        self.assertFalse(any(m == "PUT" for m,_ in calls))
        self.assertTrue(calls[-1][1].endswith("release"))

    def test_upload_failure_remains_primary_when_release_also_fails(self):
        _, _, error = self.run_seed(upload_status=503, release_status=500)
        self.assertIn("seed upload: HTTP 503", str(error))
        self.assertIn("seed release: HTTP 500", error.__notes__[0])

    def test_authenticated_redirect_is_not_followed(self):
        seen = []
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                seen.append(self.path)
                self.send_response(302)
                self.send_header("Location", f"http://localhost:{self.server.server_port}/should-not-receive-token")
                self.end_headers()
            def log_message(self, *args):
                pass
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            status, _ = _call(f"http://127.0.0.1:{server.server_port}", "GET", "/redirect", token="dummy")
            self.assertEqual(status, 302)
            self.assertEqual(seen, ["/redirect"])
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_lease_drain_wait_reuses_acquire_key_and_never_takes_over(self):
        import live_group_evidence as group
        request = {"idempotency_key": "same-key"}
        with mock.patch.object(group, "_call", side_effect=[(409, b'{}'), (200, b'{"lease": {"session_id": "ok"}}')]) as api, \
             mock.patch.object(group.time, "sleep"):
            result = _acquire_after_release("http://127.0.0.1", request, "dummy", 15)
            self.assertEqual(result, {"session_id": "ok"})
            self.assertEqual(api.call_count, 2)
            for call in api.call_args_list:
                self.assertEqual(call.args[2], "/v1/sessions/acquire-world")
                self.assertEqual(call.kwargs["payload"], request)

    def test_admission_error_does_not_retry_as_lease_drain(self):
        import live_group_evidence as group
        with mock.patch.object(group, "_call", return_value=(401, b'{}')) as api:
            with self.assertRaisesRegex(seed.HarnessFailure, "HTTP 401"):
                _acquire_after_release("http://127.0.0.1", {}, "dummy", 15)
            self.assertEqual(api.call_count, 1)


if __name__ == "__main__":
    unittest.main()
