#!/usr/bin/env python3
"""Exercise Hydra's real process launcher and ELF patching on Linux.

Build tools/patchelf and tools/mgba-rom-test-hydra first, then run:
    python3 tools/mgba-rom-test-hydra/test_sharding.py

The fixture is an ELF32 object, so no ARM compiler or emulator is needed.
"""

import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
HYDRA = ROOT / "tools/mgba-rom-test-hydra/mgba-rom-test-hydra"
PATCHELF = ROOT / "tools/patchelf/patchelf"
WORKER_RESULT = re.compile(r"^\[\d+\] worker-(\d+)-of-(\d+): PASS$", re.MULTILINE)


@unittest.skipUnless(sys.platform.startswith("linux"), "requires Linux host tools")
class ShardingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for tool in (HYDRA, PATCHELF):
            if not tool.is_file():
                raise RuntimeError(f"Build the host tool first: make -C {tool.parent}")

        temporary = tempfile.TemporaryDirectory(prefix="hydra-sharding-")
        cls.addClassCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        source = directory / "fixture.c"
        source.write_text(
            "const unsigned char gTestRunnerN = 0;\n"
            "const unsigned char gTestRunnerI = 0;\n",
            encoding="utf-8",
        )
        cls.fixture = directory / "fixture.elf"
        subprocess.run(
            shlex.split(os.environ.get("CC", "cc"))
            + ["-m32", "-c", str(source), "-o", str(cls.fixture)],
            check=True,
            capture_output=True,
            text=True,
        )

        cls.emulator = directory / "mock-emulator"
        cls.emulator.write_text(
            "#!/usr/bin/env python3\n"
            + textwrap.dedent(
                """\
                import os
                from pathlib import Path
                import signal
                import struct
                import sys

                # Read the values patched by the real tools/patchelf process.
                data = Path(sys.argv[-1]).read_bytes()
                header = struct.unpack_from("<16sHHIIIIIHHHHHH", data)
                sections = [
                    struct.unpack_from("<IIIIIIIIII", data, header[6] + i * header[11])
                    for i in range(header[12])
                ]
                values = {}
                for table in sections:
                    if table[1] != 2:  # SHT_SYMTAB
                        continue
                    strings = sections[table[6]]
                    for offset in range(table[4], table[4] + table[5], table[9]):
                        name, value, size, info, other, section = struct.unpack_from(
                            "<IIIBBH", data, offset
                        )
                        name = data[strings[4] + name:].split(b"\\0", 1)[0]
                        if name in (b"gTestRunnerN", b"gTestRunnerI"):
                            target = sections[section]
                            values[name] = data[target[4] + value - target[3]]

                total = values[b"gTestRunnerN"]
                index = values[b"gTestRunnerI"]
                if not 0 <= index < total <= 32:
                    sys.exit(90)
                behavior = os.environ.get("HYDRA_TEST_BEHAVIOR", "pass")
                if behavior == "empty":
                    sys.exit(0)
                if index == 0:
                    if behavior == "exit":
                        sys.exit(7)
                    if behavior == "signal":
                        os.kill(os.getpid(), signal.SIGTERM)
                    if behavior.startswith("report-"):
                        result, _, exit_status = behavior[7:].partition(":")
                        print(f"GBA Debug: :Nreported-{result}")
                        print("GBA Debug: :Lfixture.c:1")
                        print(f"GBA Debug: :{result}RESULT")
                        print("GBA Debug: :Nafter-restart")
                        print("GBA Debug: :ECRASH")
                        sys.exit(int(exit_status or "0"))
                print(f"GBA Debug: :Nworker-{index}-of-{total}")
                print("GBA Debug: :PPASS")
                """
            ),
            encoding="utf-8",
        )
        cls.emulator.chmod(0o755)

    def run_hydra(self, *, workers=2, count=None, index=None, behavior="pass"):
        environment = os.environ.copy()
        for name in ("TEST_SHARD_COUNT", "TEST_SHARD_INDEX", "MAKE_TERMOUT"):
            environment.pop(name, None)
        environment["MAKEFLAGS"] = f"-j{workers}"
        environment["HYDRA_TEST_BEHAVIOR"] = behavior
        if count is not None:
            environment["TEST_SHARD_COUNT"] = str(count)
        if index is not None:
            environment["TEST_SHARD_INDEX"] = str(index)
        return subprocess.run(
            [str(HYDRA), str(self.emulator), "unused-objcopy", str(self.fixture)],
            cwd=ROOT,
            env=environment,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def worker_results(self, result):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return [(int(index), int(total)) for index, total in WORKER_RESULT.findall(result.stdout)]

    def test_default_runs_every_local_worker(self):
        self.assertCountEqual(self.worker_results(self.run_hydra()), [(0, 2), (1, 2)])

    def test_shards_form_disjoint_complete_worker_set(self):
        baseline = self.worker_results(self.run_hydra(workers=4))
        first = self.worker_results(self.run_hydra(count=2, index=0))
        second = self.worker_results(self.run_hydra(count=2, index=1))
        self.assertCountEqual(first, [(0, 4), (1, 4)])
        self.assertCountEqual(second, [(2, 4), (3, 4)])
        self.assertCountEqual(first + second, baseline)

    def test_last_worker_at_maximum_total(self):
        result = self.run_hydra(workers=1, count=32, index=31)
        self.assertEqual(self.worker_results(result), [(31, 32)])

    def test_invalid_configurations_fail_before_starting_workers(self):
        configurations = (
            {"count": 0},
            {"count": 2, "index": 2},
            {"count": 33},
            {"count": "invalid"},
            {"count": -1},
            {"count": "1.5"},
            {"count": 2, "index": -1},
            {"count": 2, "index": "invalid"},
            {"workers": 17, "count": 2},
        )
        for configuration in configurations:
            with self.subTest(**configuration):
                result = self.run_hydra(**configuration)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertNotIn("worker-", result.stdout)
                self.assertTrue(result.stderr.strip())

    def test_nonzero_worker_exit_is_propagated(self):
        result = self.run_hydra(behavior="exit")
        self.assertEqual(result.returncode, 7, result.stdout + result.stderr)
        self.assertEqual(WORKER_RESULT.findall(result.stdout), [("1", "2")])

    def test_signaled_worker_fails_even_when_another_worker_passes(self):
        result = self.run_hydra(behavior="signal")
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(WORKER_RESULT.findall(result.stdout), [("1", "2")])

    def test_empty_shard_fails(self):
        result = self.run_hydra(count=2, index=1, behavior="empty")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("No tests found", result.stdout)

    def test_reported_failures_survive_a_clean_exit_after_expected_crash(self):
        for category in ("F", "U", "V"):
            with self.subTest(category=category):
                result = self.run_hydra(behavior=f"report-{category}")
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn("after-restart: CRASH", result.stdout)
                self.assertEqual(WORKER_RESULT.findall(result.stdout), [("1", "2")])

    def test_expected_failures_and_crashes_remain_successful(self):
        for category in ("E", "K"):
            with self.subTest(category=category):
                result = self.run_hydra(behavior=f"report-{category}")
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("after-restart: CRASH", result.stdout)

    def test_reported_failure_preserves_higher_worker_error(self):
        result = self.run_hydra(behavior="report-F:7")
        self.assertEqual(result.returncode, 7, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
