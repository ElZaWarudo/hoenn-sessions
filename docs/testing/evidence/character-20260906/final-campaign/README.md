# Final native campaign

Started 2026-09-07 on local branch `codex/littleroot-conformance`, base
`598de40eb16cd59793bf43a4a5c3d6028812be09` plus the reviewed local diff.
Status: native acceptance passed on 2026-09-07, including actual cleanup.
The stock-mGBA harness passed in 4221.48 seconds (one test, no failures).

Pinned SHA-256 values:

- ROM: `8d599e9742e14c4418e754f9a105299d99058c584f59a7a1b5ae35eb8b336eb0`
- Stock mGBA 0.10.5: `5a3c98c2984dd04bd0d7c9378cdfae937ae0d73a196c880bb2eecf3b254af247`
- Standalone sidecar: `e7ffb3a55b430d761085d6036ce49328b1c28547bc241f5f35a0723b21379593`
- Native harness: `5d130065c975e64bd6458aad7e5c9753ea07ea4b6b4dc903fd22e69ffefe9233`

Screenshots were retained only after direct native observation. The final
runtime was not rebuilt or reloaded during this attempt.

| Check | Evidence |
|---|---|
| Authentication and initial characters | `authenticated-player-one.jpg`, `authenticated-player-two.jpg`, `original-preview.jpg`, `wally-preview.jpg`, `leaf-preview.jpg`, `selected-player-one.jpg`, `selected-player-two.jpg` |
| Roster wraps and cancellation preserves Wally | `roster-cycled-steven-preview.jpg`, `cancel-wally-and-remote-leaf-moved.jpg` |
| Reciprocal movement | `remote-wally-moved.jpg`, `cancel-wally-and-remote-leaf-moved.jpg` |
| Ordinary active Save and presence rejoin | `save-one-rejoined-field.jpg`, `remote-wally-moved-after-save.jpg`, `remote-leaf-moved-after-save.jpg` |
| Readable Online names and empty states | `online-ungrouped-home.jpg`, `nearby-name.jpg`, `empty-invitations.jpg`, `empty-nearby.jpg` |
| Invitation accepted, both members see group | `invitation-sent.jpg`, `group-joined-player-one.jpg`, `group-joined-player-two.jpg` |
| Each member leaves; both independently ungrouped | `recipient-left.jpg`, `sender-ungrouped-after-recipient-left.jpg`, `sender-left.jpg`, `recipient-ungrouped-after-sender-left.jpg`, `remote-movement-after-both-leaves.jpg` |
| Decline and subsequent invalid accept | `invitation-declined.jpg`, `declined-accept-remains-ungrouped.jpg` |
| Live invitation expires; acceptance rejected | `invitation-before-expiry.jpg`, `expired-invitation-rejected.jpg`, `expired-accept-remains-ungrouped.jpg` |
| Loading/Back and HTTP 503/Back/Refresh | `online-delayed-loading.jpg`, `back-during-loading-field.jpg`, `online-unavailable.jpg`, `unavailable-back-field.jpg`, `online-refresh-recovered.jpg` |
| Two real socket interruptions and reciprocal recovery | `remote-wally-after-socket-interruption.jpg`, `remote-leaf-after-socket-interruption.jpg` |
| Both house entries and reciprocal returns | `wally-indoor.jpg`, `leaf-indoor.jpg`, `wally-returned-peer-visible.jpg`, `remote-wally-returned.jpg`, `leaf-returned-peer-visible.jpg`, `remote-leaf-returned.jpg`, `remote-movement-after-houses.jpg` |
| Ordinary menus and dialogue after Online | `bag-after-online.jpg`, `dialogue-after-online.jpg` |
| Full pause menu scrolling, wrap, Character, Online, Exit | `debug-pause-unlocks.jpg`, `full-pause-top.jpg`, `full-pause-bottom.jpg`, `full-pause-wrap-top.jpg`, `full-pause-scrolled.jpg`, `character-from-full-menu.jpg`, `online-from-full-menu.jpg`, `full-menu-exit-field.jpg` |

The active Save returned prepare/finalize HTTP 200, then a fresh ticket/upgrade
and two joined players; reciprocal movement was directly observed afterward.
Its transient saved-message text was not captured. Separate same-ROM offline
Continue evidence is in `../checkpoint-liveness-retest`; full cloud resume and
group travel are not certified by this campaign.

The proxy reported two established WebSocket disconnections, zero joined players,
two fresh HTTP 200 ticket mints and HTTP 101 upgrades, then two joined players.
The native observation proves reciprocal recovery. Late-response ordering is
covered by the deterministic reopened-view regression, not by a claimed native
overlap: the held response auto-released after Back.

One initial acceptance attempt stayed ungrouped, and a later decline attempt
returned the invitation-domain HTTP 401 expiry result. Neither was counted as a
pass. Fresh views and pre-staged shorter recipient input established successful
acceptance and decline. Expiry was then tested separately from an observed live
invitation after waiting beyond its 30-second lifetime.

Acceptance was written only after the gameplay assertions had been observed.
The harness then reported both lifecycle drains and lease releases, retained the
genuine SAV files after child shutdown, and exited successfully. Temporary fault
markers belonged to this attempt's temporary directory and were cleaned with it.
Private retained saves: player one `coop-real-save-player-1-NEV1OR/character.sav`,
player two `coop-real-save-player-2-IUEljq/character.sav` under the operator's Temp
directory. Player two did not perform an ordinary Save in this attempt; its
retained file is not proof of Leaf persistence.
