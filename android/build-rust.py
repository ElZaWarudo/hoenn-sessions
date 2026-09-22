"""Build the pinned Android Rust libraries with the installed NDK on Linux/Windows."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
MGBA_COMMIT = "26b7884bc25a5933960f3cdcd98bac1ae14d42e2"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--sdk", required=True)
    args = parser.parse_args()
    mgba = ROOT / ".local/mgba"
    for command, expected in [(["rev-parse", "HEAD"], MGBA_COMMIT), (["status", "--porcelain"], "")]:
        actual = subprocess.check_output(["git", "-C", str(mgba), *command], text=True).strip()
        if actual != expected:
            raise SystemExit("mGBA must be the pinned clean checkout")
    windows = sys.platform == "win32"
    host = "windows-x86_64" if windows else "linux-x86_64"
    suffix = ".exe" if windows else ""
    llvm = Path(args.sdk) / "ndk/27.2.12479018/toolchains/llvm/prebuilt" / host / "bin"
    if not (llvm / f"clang{suffix}").is_file():
        raise SystemExit("Install NDK 27.2.12479018 first")
    for triple, abi in [("x86_64-linux-android", "x86_64"), ("aarch64-linux-android", "arm64-v8a")]:
        subprocess.run(["rustup", "target", "add", triple], check=True)
        env = os.environ.copy()
        lower = triple.replace("-", "_")
        env[f"CARGO_TARGET_{lower.upper()}_LINKER"] = str(llvm / f"clang{suffix}")
        env[f"CARGO_TARGET_{lower.upper()}_RUSTFLAGS"] = f"-C link-arg=--target={triple}28 -C link-arg=-Wl,-z,max-page-size=16384"
        env[f"CC_{lower}"] = str(llvm / f"clang{suffix}")
        env[f"CFLAGS_{lower}"] = f"--target={triple}28"
        env[f"AR_{lower}"] = str(llvm / f"llvm-ar{suffix}")
        subprocess.run(["cargo", "build", "-p", "coop-android", "--target", triple, "--release", "--locked"], cwd=ROOT, env=env, check=True)
        output = ROOT / "android/app/build/generated/rustJniLibs" / abi
        output.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / "target" / triple / "release/libcoop_android.so", output)


if __name__ == "__main__":
    main()
