![Pokémon Crossroads logo](crossroads_logo.png)

# Hoenn Sessions

**Hoenn Sessions** is an independent development fork of
[Pokémon Crossroads](https://github.com/eonlynx/pokecrossroads) that explores a
private, two-player online cooperative mode for the Game Boy Advance game.
Pokémon Crossroads supplies the multi-region adventure; this fork adds the
protocol, ROM integration, emulator bridge, launcher, and server foundations
needed for two players to share that world.

> **Development status:** this repository contains tested local engineering
> checkpoints, not a ready-to-install multiplayer release. There is no public
> Hoenn Sessions server or packaged player launcher yet. To play the original
> single-player Pokémon Crossroads Beta 1.4, use the
> [official upstream release](https://github.com/eonlynx/pokecrossroads/releases/tag/Beta-1.4).

## The idea

Two players keep separate characters, parties, and saves while appearing in the
same online world. Neither player is the host or group leader. The service owns
shared session state, invitations, travel decisions, and save revisions; the
ROM remains responsible for Pokémon mechanics and the actual game simulation.

Hoenn is the first campaign slice, but identities and progress are designed for
all four areas represented by the project: Hoenn, Kanto, Johto, and the Sevii
Islands. Badges, trainers, events, Fly points, and story progress are qualified
by region so that progress from one region cannot accidentally unlock or alter
another.

## Relationship to Pokémon Crossroads

This is a GitHub fork, not the official Pokémon Crossroads repository.

- **Upstream project:** [eonlynx/pokecrossroads](https://github.com/eonlynx/pokecrossroads)
- **Pinned game base:** Beta 1.4 at commit
  [`e05c828`](https://github.com/eonlynx/pokecrossroads/commit/e05c82865d38a6638173fd30b2c830d1250aa50d)
- **Pokémon Crossroads developers:** eonlynx and justgoose
- **Upstream discussion:** [PokéCommunity thread](https://www.pokecommunity.com/threads/pok%C3%A9mon-crossroads-kanto-johto-and-hoenn-joined.536507/)
- **Upstream community:** [Pokémon Crossroads Discord](https://discord.gg/ReWmTP86Ap)

Pokémon Crossroads joins Hoenn, Kanto, and the Sevii Islands in one Emerald-based
adventure, with Johto under development. Its Beta 1.4 includes the Emerald and
FireRed storylines, cross-region travel, and 16 obtainable Gym Badges. Hoenn
Sessions preserves that work and its Git history, then layers the co-op systems
on top.

## What is implemented

The following pieces are implemented and covered by repository tests or
recorded local validation. They should be read as engineering checkpoints, not
as a claim that the complete product is ready for public play.

- A region-safe Rust protocol shared by the launcher, sidecar, and service.
- Account registration, login, exclusive character leases, and signed resume
  packages in the authenticated local service mode.
- Byte-accurate handling of Pokémon Crossroads' 128 KiB Flash1M saves, including
  revision checks, lineage checks, regional progress, and optional compatible
  savestates.
- Symmetric two-member groups with invitations and atomic group travel across
  configured Hoenn, Kanto, and Sevii routes.
- An authenticated realtime presence path using bounded HTTP and WebSocket
  exchanges, single-use tickets, and explicit session epochs.
- ROM-side publication of local position and rendering of one safe,
  nonblocking remote Brendan or May avatar, including interpolation, warps,
  disappearance, and adjacent interaction observations.
- A supervised Windows launcher core, local Rust sidecar, and mGBA Lua bridge
  with strict loopback-only control boundaries and generated ROM address
  manifests.

The latest presence work is recorded in the
[Phase 5 closeout](docs/orchestration/runs/pokecrossroads-phase5-presence-activation-20260902/phase5-presence-resume-final-closeout.json).

## How the pieces fit together

```text
Pokémon Crossroads ROM
        ↕ NetBridge memory ABI
mGBA 0.10.5 + Lua bridge
        ↕ authenticated loopback connection
Rust launcher + local sidecar
        ↕ authenticated HTTP / WebSocket
Rust co-op service
```

The launcher validates the selected ROM and supported mGBA executable before a
session begins. The Lua bridge moves bounded messages between emulated memory
and the local sidecar. The sidecar translates those messages into authenticated
service traffic without putting account credentials or service tickets inside
the ROM.

See [the co-op workspace guide](coop/README.md) and
[the mGBA bridge guide](bridge/README.md) for the detailed boundaries and test
flows.

## What remains before a public release

- Production PostgreSQL and object-storage adapters, deployment, backup, and
  garbage-collection operations.
- A packaged graphical launcher and a supported player onboarding flow.
- Full interactive conformance testing with the pinned stock mGBA build, the
  Lua bridge, two live game sessions, checkpoints, reconnects, and shutdown.
- Server-authorized gameplay progression and deterministic cooperative battle
  synchronization.
- Resolution of the remaining local process-hardening and first-save enrollment
  risks.
- Clear upstream permission or licensing terms for redistribution.

The maintained list of open engineering and distribution constraints is in
[`docs/swarm/blockers.yaml`](docs/swarm/blockers.yaml).

## Development setup

Clone this fork and keep the original project available as `upstream`:

```bash
git clone https://github.com/ElZaWarudo/hoenn-sessions.git
cd hoenn-sessions
git remote add upstream https://github.com/eonlynx/pokecrossroads.git
```

### Build the ROM

You need a compatible `arm-none-eabi` toolchain. Follow
[`INSTALL.md`](INSTALL.md) for the operating-system-specific setup, then build
the modern target:

```bash
make modern
```

The build produces `pokeemerald.gba`. ROMs, BIOS files, saves, savestates, and
generated private session files must not be committed. Use only game material
you obtained legally.

### Validate the co-op workspace

The Rust workspace currently targets Rust 1.93.1. From the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
python -m unittest discover -s tools/tests -p "test_*.py"
```

The bounded Phase 2 integration smoke uses local adapters and publishes no
service to the network:

```bash
python tools/smoke_phase2.py --local
```

Running the complete emulator path also requires the pinned official mGBA
0.10.5 Windows x64 Qt build, a matching locally built ROM, a generated bridge
manifest, and manual loading of `bridge/main.lua`. The bridge guide documents
that process and its current limitations.

## Credits and project identity

Hoenn Sessions exists because of the work already present in Pokémon
Crossroads and pokeemerald-expansion. Preserving the upstream commit history is
intentional: authorship should remain visible at the commit and file level.

- **Pokémon Crossroads:** eonlynx, justgoose, and the Crossroads Dev Team
- **Game base:** [pokeemerald-expansion](https://github.com/rh-hideout/pokeemerald-expansion)
- **Engine logic:** cawtds for importing FireRed logic into Emerald
- **Travel system:** AsparagusEduardo for the Kanto/Hoenn travel work
- **Sprites:** @h y o for the Gold / Ethan sprites
- **Hoenn Sessions co-op work:** ElZaWarudo and contributors to this fork

The full inherited contributor list is preserved in [`CREDITS.md`](CREDITS.md).

The pinned Pokémon Crossroads revision does not contain a top-level license.
This fork's history and credits record provenance, but they do not create new
redistribution rights. Confirm permission with the relevant upstream
maintainers before redistributing source, assets, patches, or binaries.

Pokémon and related properties belong to their respective owners. This is an
unofficial fan project and is not affiliated with Nintendo, Creatures Inc.,
Game Freak, or The Pokémon Company.

## Reporting issues

Use [this fork's issue tracker](https://github.com/ElZaWarudo/hoenn-sessions/issues)
for Hoenn Sessions launcher, server, bridge, save, or multiplayer work. Report
issues that reproduce in the unmodified Pokémon Crossroads release to the
[upstream issue tracker](https://github.com/eonlynx/pokecrossroads/issues).
