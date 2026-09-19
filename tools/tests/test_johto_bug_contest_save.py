import hashlib
import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[2]
DONOR = Path(r"C:/Users/Mayor/Documents/Caribbean/johto-hns")


def normalized_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest()


def function_body(source, name):
    match = re.search(r"\b" + re.escape(name) + r"\([^;{}]*\)\s*\{", source)
    if not match:
        raise AssertionError("missing function " + name)
    start = match.end()
    depth = 1
    for offset in range(start, len(source)):
        depth += (source[offset] == "{") - (source[offset] == "}")
        if depth == 0:
            return source[start:offset]
    raise AssertionError("unclosed function " + name)


class JohtoBugContestSaveFenceTests(unittest.TestCase):
    def setUp(self):
        self.contest = (ROOT / "src/johto/bug_contest.c").read_text()
        self.header = (ROOT / "include/johto/bug_contest.h").read_text()
        self.save = (ROOT / "src/save.c").read_text()
        self.menu = (ROOT / "src/start_menu.c").read_text()
        self.bridge = (ROOT / "src/coop/net_bridge.c").read_text()
        self.fixture = (ROOT / "test/johto/bug_contest_save.c").read_text()

    def test_public_fence_is_the_real_contest_lifetime(self):
        self.assertIn("bool32 JohtoBugContest_IsSerializationBlocked(void);", self.header)
        self.assertRegex(
            self.contest,
            re.compile(
                r"bool32 JohtoBugContest_IsSerializationBlocked\(void\)\s*"
                r"\{.*?return sBugContest != NULL;.*?\}",
                re.S,
            ),
        )
        self.assertNotIn("sBugContestTestActive", self.contest)

    def test_save_paths_fence_before_copy_or_flash(self):
        self.assertIn('#include "johto/bug_contest.h"', self.save)
        handle = self.save[self.save.index("u8 HandleSavingData(u8 saveType)"):self.save.index("u8 TrySavingData(u8 saveType)")]
        try_save = self.save[self.save.index("u8 TrySavingData(u8 saveType)"):self.save.index("bool8 LinkFullSave_Init(void)")]
        link_init = self.save[self.save.index("bool8 LinkFullSave_Init(void)"):self.save.index("bool8 LinkFullSave_WriteSector(void)")]
        self.assertLess(handle.index("CancelSaveForContest"), handle.index("CopyPartyAndObjectsToSave"))
        self.assertLess(try_save.index("CancelSaveForContest"), try_save.index("HandleSavingData"))
        self.assertLess(link_init.index("CancelSaveForContest"), link_init.index("CopyPartyAndObjectsToSave"))
        self.assertIn("JohtoBugContest_IsSerializationBlocked", function_body(self.save, "CancelSaveForContest"))
        self.assertIn("return SAVE_STATUS_ERROR;", try_save)
        blocked = try_save[try_save.index("CancelSaveForContest"):try_save.index("if (gFlashMemoryPresent")]
        self.assertNotIn("DoSaveFailedScreen", blocked)
        for name in ("LinkFullSave_WriteSector", "LinkFullSave_ReplaceLastSector", "LinkFullSave_SetLastSectorSignature", "WriteSaveBlock2", "WriteSaveBlock1Sector"):
            self.assertIn("CancelSaveForContest", function_body(self.save, name))

    def test_menu_and_checkpoint_guards_cancel_cleanly(self):
        save_callback = self.menu[self.menu.index("static u8 SaveDoSaveCallback(void)\n{"):self.menu.index("static u8 SaveCheckpointWaitCallback(void)\n{")]
        authorized_start = self.menu.index("static u8 SaveDoSaveAuthorizedCallback(void)\n{")
        authorized_end = self.menu.index("\n#if TESTING\nvoid CoopStartMenu_TestSetSaveDryRun", authorized_start)
        authorized = self.menu[authorized_start:authorized_end]
        self.assertLess(save_callback.index("JohtoBugContest_IsSerializationBlocked"), save_callback.index("CoopNetBridge_RequestCheckpoint"))
        self.assertLess(authorized.index("JohtoBugContest_IsSerializationBlocked"), authorized.index("IncrementGameStat"))
        request = self.bridge[self.bridge.index("enum CoopCheckpointRequestResult CoopNetBridge_RequestCheckpoint"):self.bridge.index("bool8 CoopNetBridge_ConsumeCheckpointGrant")]
        self.assertLess(request.index("JohtoBugContest_IsSerializationBlocked"), request.index("!sCoopNetRuntime.cloud_epoch_accepted"))
        self.assertIn("COOP_CHECKPOINT_REQUEST_REJECTED", request)

    def test_c_fixture_uses_real_lifecycle_and_noop_flash_oracles(self):
        for token in (
            "JohtoBugContest_Begin(100)",
            "JohtoBugContest_RequestEnd(JOHTO_BUG_CONTEST_END_RETIRE)",
            "HandleSavingData(SAVE_NORMAL)",
            "TrySavingData(SAVE_NORMAL)",
            "LinkFullSave_Init()",
            "Save_TestSetFlashProgramCallback",
            "CoopStartMenu_TestRunSaveDoSaveCallback",
            "JohtoBugContest_Abort()",
            "COOP_CHECKPOINT_REQUEST_OFFLINE",
        ):
            self.assertIn(token, self.fixture)
        self.assertIn("gSaveBlock1Ptr->playerParty", self.fixture)
        self.assertIn("sProgramCalls, 0", self.fixture)
        self.assertGreaterEqual(self.fixture.count("TEST("), 2)

    def test_provenance_tracks_normalized_core_and_completed_campaign_hooks(self):
        provenance = json.loads((ROOT / "data/johto/bug_contest.json").read_text())
        donor = DONOR / "src/bug_contest.c"
        self.assertEqual(provenance["provenance"]["donor_revision"], "751823abaf677020bcd72c45fe3e7cb2b8a576e4")
        self.assertEqual(provenance["provenance"]["donor_source_sha256"], hashlib.sha256(donor.read_bytes()).hexdigest())
        self.assertEqual(provenance["provenance"]["target_source_sha256"], normalized_sha256(ROOT / "src/johto/bug_contest.c"))
        self.assertTrue(provenance["engine_hooks_ready"])
        self.assertTrue(provenance["campaign_ready"])
        self.assertTrue(any("Serialization fence" in item for item in provenance["adaptations"]))


if __name__ == "__main__":
    unittest.main()
