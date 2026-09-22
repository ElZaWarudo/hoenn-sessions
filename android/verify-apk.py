"""Verify the shipped ROM, manifest and both native ABIs, not just build inputs."""
import hashlib
import json
from pathlib import Path
import sys
import zipfile

apk, manifest_path = map(Path, sys.argv[1:])
manifest_bytes = manifest_path.read_bytes()
manifest = json.loads(manifest_bytes)
with zipfile.ZipFile(apk) as archive:
    assert archive.read("assets/bridge_manifest.json") == manifest_bytes, "APK manifest mismatch"
    rom_hash = hashlib.sha256(archive.read("assets/pokeemerald.gba")).hexdigest()
    assert rom_hash == manifest["game_build"]["rom_sha256"], "APK ROM mismatch"
    for abi in ("arm64-v8a", "x86_64"):
        for library in ("libhoenn.so", "libcoop_android.so"):
            assert archive.getinfo(f"lib/{abi}/{library}").file_size > 0
print(f"Verified APK ROM {rom_hash}, manifest and ARM64/x86_64 native libraries")
