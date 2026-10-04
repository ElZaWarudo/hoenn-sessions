"""Guard the donor casino song and its two-ROM song identity."""

from __future__ import annotations

import hashlib
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MIDI = ROOT / "sound/songs/midi/mus_hgss_casino.mid"
DONOR_SHA256 = "aec0f4903d27b10b2dd5bce388521a4fe8943690a51052c2690e875910a9e0b3"
CASINO_PLUS_MIDI = ROOT / "sound/songs/midi/mus_casino_plus_1.mid"
CASINO_PLUS_SHA256 = "6b8efa36fcb18d218221e6d4379ee3f83969fe6d5ab9b40b381c7c0f763ca900"
CASINO_PLUS_SAMPLES = {
    "pinball_7_clav": "4d144356ff94ebffdd8f6cd2ca7446ec69dfe0f74bd044dcd7c6b1a70caf25c4",
    "pinball_11_vibraphone": "f1e3d12ec274b76a86e36daef384979c2f0ac7baa26b29aeac8cddb833cb50ab",
    "pinball_12_marimba": "d643ec12266361c36136dc36423b565735ffa5a604470010dcd65d301df7a300",
    "pinball_16_drawbar_organ": "763a90c7d4e5c6884bc7366eab9f2e396758333ea104e56a3c3bb549adec4175",
    "pinball_52_clarinet": "d8e317fd46258e5c93a361c09cf9e9a0708286712ad28899605128bbf6fcc063",
}


class CormoriaCasinoMusicTests(unittest.TestCase):
    def test_pinned_donor_midi_is_installed(self) -> None:
        data = MIDI.read_bytes()
        self.assertEqual(hashlib.sha256(data).hexdigest(), DONOR_SHA256)
        self.assertTrue(data.startswith(b"MThd"))

    def test_song_id_is_appended_and_main_has_safe_fallback(self) -> None:
        constants = (ROOT / "include/constants/songs.h").read_text()
        self.assertRegex(constants, r"(?m)^#define MUS_HGSS_CASINO\s+610$")
        self.assertRegex(constants, r"(?m)^#define MUS_CASINO_PLUS_1\s+611$")
        self.assertRegex(constants, r"(?m)^#define PH_NURSE_SOLO\s+609$")

        table = (ROOT / "sound/song_table.inc").read_text()
        entries = re.findall(r"(?m)^\s*song\s+(\w+),", table)
        # Each conditional pair occupies one slot in either ROM.
        self.assertEqual(entries[609:614],
                         ["ph_nurse_solo", "mus_hgss_casino", "mus_casino_plus_1",
                          "dummy_song_header", "dummy_song_header"])
        self.assertRegex(
            table,
            r"\.if ROM_WORLD == 2\s+song mus_hgss_casino, MUSIC_PLAYER_BGM, 0"
            r"\s+song mus_casino_plus_1, MUSIC_PLAYER_BGM, 0"
            r"\s+\.else\s+song dummy_song_header, MUSIC_PLAYER_BGM, 0"
            r"\s+song dummy_song_header, MUSIC_PLAYER_BGM, 0\s+\.endif",
        )

    def test_donor_arrangement_is_only_built_in_cormoria(self) -> None:
        config = (ROOT / "sound/songs/midi/midi.cfg").read_text()
        self.assertRegex(
            config,
            r"(?m)^mus_hgss_casino\.mid:\s+-E -R50 -G_cormoria_hgss_casino -V086$",
        )
        makefile = (ROOT / "Makefile").read_text()
        self.assertIn("$(MID_SUBDIR)/mus_hgss_casino.mid $(MID_SUBDIR)/mus_casino_plus_1.mid", makefile)
        voices = (ROOT / "sound/voice_groups.inc").read_text()
        self.assertRegex(
            voices,
            r"\.if ROM_WORLD == 2\s+\.include "
            r'"sound/voicegroups/cormoria_hgss_casino_drumset.inc"\s+'
            r'\.include "sound/voicegroups/cormoria_hgss_casino.inc"\s+'
            r'\.include "sound/voicegroups/cormoria_casino_plus_1.inc"\s+\.endif',
        )

    def test_casino_plus_track_and_instruments_are_pinned(self) -> None:
        self.assertEqual(hashlib.sha256(CASINO_PLUS_MIDI.read_bytes()).hexdigest(),
                         CASINO_PLUS_SHA256)
        voices = (ROOT / "sound/voicegroups/cormoria_casino_plus_1.inc").read_text()
        self.assertIn("voice_group cormoria_casino_plus_1", voices)
        self.assertIn("voice_keysplit_all voicegroup_cormoria_hgss_casino_drumset", voices)
        samples = (ROOT / "sound/direct_sound_data.inc").read_text()
        for name, donor_hash in CASINO_PLUS_SAMPLES.items():
            sample = ROOT / "sound/direct_sound_samples" / f"{name}.bin"
            self.assertEqual(hashlib.sha256(sample.read_bytes()).hexdigest(), donor_hash)
            self.assertIn(f"DirectSoundWaveData_{name}::", samples)
            self.assertIn(f"DirectSoundWaveData_{name}", voices)
        self.assertIn("mus_casino_plus_1.mid:         -E -R50 -G_cormoria_casino_plus_1 -V090",
                      (ROOT / "sound/songs/midi/midi.cfg").read_text())


if __name__ == "__main__":
    unittest.main()
