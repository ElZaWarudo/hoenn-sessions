# Johto Seneschal startup

Run: `johto-region-20260908`. Operation: `plan`. This namespace is intentionally
separate from the existing cloud-coop queue and roadmap.

Source: `a5ace3186f38e147b31a9cd3cb4a660ae71f0afa` on
`codex/johto-new-bark`. Donor: HnS `751823abaf677020bcd72c45fe3e7cb2b8a576e4`.
All participants inherit [the initiative contract](../../plans/johto-region/initiative-requirements.md).
Scope: complete Johto and its Kanto connections. No mutable worker launched.

## Gate and authority

The user accepted this packet on 2026-09-08 and requested a fully autonomous,
subagentic goal. Root binds that approval to the current artifact bytes before
dispatch. No historical cloud-coop gate bypass is inherited. Local work,
verification and necessary scoped local commits are authorized. Perform one
release phase at the end. Jira, push, PR, merge, deployment, publication and PC
scheduling remain outside this run.

## Ownership and workers

Root owns the initiative, queue, dependency graph, authoritative verification
and reconciliation. Each future role receives a new detached worktree based
on the recorded source plus accepted dependency patches. Mutable concurrency
is capped at two; shared save, generated, protocol and registry surfaces are
serialized. Workers cannot edit the canonical checkout, index, branches or
queue, and cannot commit, push or release.

J1 is deep/high because save storage, numeric identity and engine compatibility
cross existing regions. Its implementation plan is incomplete, so a nested
Compound Master artifact-planning run is appropriate after approval:

```text
krt-compound-master docs/plans/johto-region/roadmap.md mode:artifacts orchestrator:seneschal run-id:johto-region-j1 state-path:docs/orchestration/compound-master/johto-region-j1/state.md initiative-contract:docs/plans/johto-region/initiative-requirements.md interaction:brokered jira-policy:skip production:unknown parallel:false delegation:auto worktree-policy:required
```

Assignment: J1 only, all initiative invariants, no code until its reviewed
implementation plan and bounded contracts exist, no unrelated roadmap, no
shipping. The child returns decisions to root. It must not read or claim the
existing cloud-coop canonical state. The first planning invocation gets a
45-minute/80-action budget, stops with a concrete plan or a named blocker,
and never silently broadens its writable paths. Other children are queued
only after their dependencies and contracts are admitted.

## Verification and release

Root observes actual diffs and enforces owned paths, then runs the selected
focused checks and one aggregate verification for each reconciled wave.
High-assurance work gets the relevant save/compatibility specialist and an
independent validator; story playability requires real emulator evidence.
Root's disagreeing result overrides worker prose. Passing build/tests do not
certify the requested complete campaign.

Release Marshal owns any eventual local commit handoff. Remote shipping is
not requested. Preserve user WIP and exclude it from Johto commits unless the
user explicitly includes the collided files. Do not reset/stash/discard it.

## Stops and resume

Stop only the affected unit for an unresolved save migration, insufficient
ROM/RAM, ambiguous Kanto behavior, unowned writes or failed verification.
Retain failed workspaces for diagnosis. Cleanup only canonical cleanup-ready
entries after dry-run inspection and durable patch/evidence capture.

Existing Kanto is the accepted connection target. Approval binds final artifact
bytes; no mutable execution state or release-ready fact is hand-edited.
