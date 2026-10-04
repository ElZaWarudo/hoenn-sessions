import hashlib
import json
import os
from pathlib import Path

import pytest

from tools.cormoria import import_world, materialize_tile_animations as animations


STAGE = Path(os.environ.get(
    "CORMORIA_STAGE",
    Path.home() / ".codex/cormoria-swarm-artifacts/content-stage-20260923-v5",
))


@pytest.mark.skipif(not STAGE.exists(), reason="authenticated donor stage is external")
def test_six_animation_frames_match_both_manifests():
    frames = animations.authenticated_frames(STAGE)
    assert len(frames) == 6
    assert all(path.as_posix().startswith(animations.TARGET_PREFIX) for path in frames)
    assert [len(data) for data in frames.values()] == [245] * 5 + [232]


@pytest.mark.skipif(not STAGE.exists(), reason="authenticated donor stage is external")
def test_tampered_animation_is_rejected(tmp_path):
    region, _, _ = import_world.load_manifests(animations.ROOT)
    rows = []
    for frame in range(6):
        relative = f"source/{animations.DONOR_PREFIX}/light_anim_{frame}.png"
        data = (STAGE / relative).read_bytes()
        target = tmp_path / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        rows.append({"path": relative, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    (tmp_path / "staging_manifest.json").write_text(json.dumps({
        "manifest_sha256": import_world.PINNED_SHA256,
        "provenance": region["provenance"],
        "files": rows,
    }), encoding="utf-8")
    bad = tmp_path / rows[2]["path"]
    bad.write_bytes(bad.read_bytes() + b"bad")
    with pytest.raises(animations.AnimationSourceError, match="hash mismatch"):
        animations.authenticated_frames(tmp_path)


def test_install_rejects_symlink_target(tmp_path, monkeypatch):
    external = tmp_path.parent / f"{tmp_path.name}-external.png"
    external.write_bytes(b"original")
    relative = Path(animations.TARGET_PREFIX) / "light_anim_0.png"
    target = tmp_path / relative
    target.parent.mkdir(parents=True)
    try:
        target.symlink_to(external)
    except (OSError, NotImplementedError):
        pytest.skip("symlinks are unavailable")
    monkeypatch.setattr(animations, "authenticated_frames", lambda stage, root: {relative: b"replacement"})
    with pytest.raises(animations.AnimationSourceError, match="escapes repository"):
        animations.install(tmp_path, tmp_path)
    assert external.read_bytes() == b"original"
