# Checkpoint liveness diagnosis and appearance reload

Native diagnostic attempt on 2026-09-07: failed after 1547.25 seconds; no
acceptance marker. Both supervised children stopped and both leases released.
ROM SHA-256: `8d599e9742e14c4418e754f9a105299d99058c584f59a7a1b5ae35eb8b336eb0`.

The first indoor Save displayed the saved-game message and its prepare/finalize
requests returned HTTP 200. Both players then joined presence, selected Wally
and Leaf, and player two entered Online. Player one's outdoor overwrite logged:

```text
Realtime checkpoint entered; active=true
Realtime driver completed: ProtocolViolation
Local presence service: 1 joined players
Local checkpoint: prepare; HTTP 200
Local checkpoint: finalize; HTTP 200
Realtime checkpoint rejected after save; terminal=true, reset=false, generation_matches=true
```

The harness retained player one's genuine 131088-byte SAV after teardown and
before lease release. SHA-256:
`8c5befe82de8015cc717a3d24e3c091142534ba1031e72e79efa82fdd4126222`.
An untouched copy paired with the same ROM was opened in stock mGBA offline.
Continue restored Wally in Littleroot; a normal leftward move remained Wally.
Screenshots record the successful indoor save, hidden peer, Continue menu, and
restored character. Only ordinary key inputs were used; the SAV was not patched.

This proves appearance persistence, not successful multiplayer save recovery.
The explicit checkpoint suspension correction still needs its final native run.
