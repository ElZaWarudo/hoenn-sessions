from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "regional_catalog", ROOT / "tools/coop/generate_regional_catalog.py"
)
assert SPEC is not None and SPEC.loader is not None
catalog = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(catalog)


class JohtoCatalogTests(unittest.TestCase):
    def setUp(self) -> None:
        self.sections = {
            "MAPSEC_LITTLEROOT_TOWN": 0,
            "MAPSEC_PALLET_TOWN": 1,
            "MAPSEC_ONE_ISLAND": 2,
            "MAPSEC_SPECIAL_AREA": 3,
            "MAPSEC_NEW_BARK_TOWN": 4,
            "MAPSEC_JOHTO_CHERRYGROVE_CITY": 5,
            "MAPSEC_JOHTO_SS_AQUA": 44,
        }

    def region(self, engine: str, section: str) -> str:
        return catalog.protocol_region(
            engine, section, self.sections, {"MAPSEC_ONE_ISLAND"}, "MAPSEC_SPECIAL_AREA"
        )

    def test_new_bark_requires_explicit_johto(self) -> None:
        self.assertEqual(self.region("REGION_JOHTO", "MAPSEC_NEW_BARK_TOWN"), "Johto")
        self.assertEqual(self.region("REGION_JOHTO", "MAPSEC_JOHTO_CHERRYGROVE_CITY"), "Johto")
        for engine in ("REGION_HOENN", "REGION_KANTO"):
            with self.subTest(engine=engine), self.assertRaises(catalog.CatalogError):
                self.region(engine, "MAPSEC_NEW_BARK_TOWN")

    def test_johto_rejects_unregistered_and_other_region_sections(self) -> None:
        for section in (*self.sections, "MAPSEC_NONE", "MAPSEC_UNKNOWN", ""):
            if section in {"MAPSEC_NEW_BARK_TOWN", "MAPSEC_JOHTO_CHERRYGROVE_CITY", "MAPSEC_JOHTO_SS_AQUA"}:
                continue
            with self.subTest(section=section), self.assertRaises(catalog.CatalogError):
                self.region("REGION_JOHTO", section)
        del self.sections["MAPSEC_NEW_BARK_TOWN"]
        with self.assertRaises(catalog.CatalogError):
            self.region("REGION_JOHTO", "MAPSEC_NEW_BARK_TOWN")

    def test_existing_regions_keep_their_authority(self) -> None:
        for engine, section, expected in (
            ("REGION_HOENN", "MAPSEC_LITTLEROOT_TOWN", "Hoenn"),
            ("REGION_KANTO", "MAPSEC_PALLET_TOWN", "Kanto"),
            ("REGION_KANTO", "MAPSEC_ONE_ISLAND", "Sevii"),
            ("REGION_KANTO", "MAPSEC_SPECIAL_AREA", "Kanto"),
            ("REGION_HOENN", "MAPSEC_SPECIAL_AREA", "Hoenn"),
        ):
            with self.subTest(engine=engine, section=section):
                self.assertEqual(self.region(engine, section), expected)


@unittest.skipUnless(shutil.which("g++"), "mapjson integration needs g++")
class JohtoMapHeaderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.temp = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temp.cleanup)
        cls.directory = Path(cls.temp.name)
        cls.executable = cls.directory / "mapjson.exe"
        subprocess.run(
            ["g++", "-std=c++17", "-O0", "mapjson.cpp", "json11.cpp", "-o", str(cls.executable)],
            cwd=ROOT / "tools/mapjson", check=True, capture_output=True, text=True,
        )

    def generate(self, engine: str | None, section: str | None) -> subprocess.CompletedProcess:
        source = json.loads((ROOT / "data/maps/LittlerootTown/map.json").read_text())
        if engine is not None:
            source["region"] = engine
        if section is None:
            source.pop("region_map_section")
        else:
            source["region_map_section"] = section
        source_path = self.directory / "map.json"
        source_path.write_text(json.dumps(source))
        layouts_path = self.directory / "layouts.json"
        layouts_path.write_text(json.dumps(json.loads(
            (ROOT / "data/layouts/layouts.json").read_text()
        )))
        return subprocess.run(
            [str(self.executable), "map", "emerald", source_path.as_posix(),
             layouts_path.as_posix(), self.directory.as_posix()],
            cwd=ROOT, capture_output=True, text=True,
        )

    def test_emitted_header_bytes_are_stable(self) -> None:
        for engine, section, ordinal in (
            (None, "MAPSEC_LITTLEROOT_TOWN", 0),
            ("REGION_HOENN", "MAPSEC_LITTLEROOT_TOWN", 0),
            ("REGION_KANTO", "MAPSEC_PALLET_TOWN", 1),
            ("REGION_JOHTO", "MAPSEC_NEW_BARK_TOWN", 2),
        ):
            with self.subTest(engine=engine):
                result = self.generate(engine, section)
                self.assertEqual(result.returncode, 0, result.stderr)
                header = (self.directory / "header.inc").read_text()
                self.assertIn(f"\t.byte {ordinal}\n\tmap_header_flags", header)

    def test_invalid_johto_headers_fail(self) -> None:
        for engine, section in (
            (None, "MAPSEC_NEW_BARK_TOWN"),
            ("REGION_HOENN", "MAPSEC_NEW_BARK_TOWN"),
            ("REGION_KANTO", "MAPSEC_NEW_BARK_TOWN"),
            ("REGION_JOHTO", "MAPSEC_LITTLEROOT_TOWN"),
            ("REGION_JOHTO", "MAPSEC_UNKNOWN"),
            ("REGION_JOHTO", None),
            ("REGION_UNKNOWN", "MAPSEC_NEW_BARK_TOWN"),
        ):
            with self.subTest(engine=engine, section=section):
                self.assertNotEqual(self.generate(engine, section).returncode, 0)


if __name__ == "__main__":
    unittest.main()
