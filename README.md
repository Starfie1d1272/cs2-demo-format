# cs2-demo-format

**Language:** English | [简体中文](./README.zh-CN.md)

`cs2-demo-format` is a strict, implementation-neutral ZIP data contract for
parsed Counter-Strike 2 demos.

It is not a demo parser, rating model, replay viewer, or web app. It is the
shared data layer those tools can agree on: one producer exports a valid ZIP,
and many consumers can validate, inspect, replay, score, or research the same
match without copying each other's private data shapes.

## Why This Exists

Raw `.dem` files are hard to consume directly. Demo parsers expose useful data,
but every downstream project tends to invent its own shape for rounds, players,
kills, damage, economy, replay frames, and derived stats.

This repository defines that shared shape:

- **A ZIP package layout** for parsed CS2 demo data.
- **Zod schemas and TypeScript types** for JavaScript/TypeScript consumers.
- **Generated JSON Schema** for Python, Go, Rust, and other tools.
- **A strict parser and validators** so packages fail loudly when malformed.
- **A reference Python exporter** (`cs2df`) that turns `.dem` files into v3 ZIPs.
- **A real v3 fixture** you can inspect without exporting a demo yourself.

The contract lives in [`schemas/index.ts`](./schemas/index.ts) and the generated
[`spec/*.schema.json`](./spec/) files. Any producer that emits a valid package is
conformant.

## What Is in a v3 ZIP?

A v3 package is a ZIP file with `manifest.json` plus JSON files declared by the
manifest.

```text
match.zip
├── manifest.json
├── match.json
├── players.json
├── rounds.json
├── player-stats.json
├── player-economies.json
├── kills.json
├── damages.json
├── blinds.json
├── bombs.json
├── grenades.json
├── clutches.json
├── shots.json      optional
├── replay.json     optional
└── duels.json      optional, research profile
```

The files fall into four groups:

| Group | Files | Purpose |
|---|---|---|
| Match identity | `manifest`, `match`, `players`, `rounds` | Stable match, player, team, side, and round timeline facts. |
| Events | `kills`, `damages`, `blinds`, `bombs`, `grenades`, `clutches` | Formal-round event rows using `playerIndex` references. |
| Aggregates | `player-stats`, `player-economies` | Per-player match stats and per-round economy snapshots. |
| Streams | `shots`, `replay`, `duels` | Columnar integer streams for fire events, 8 Hz replay, and full-tick combat windows. |

For every field and calculation rule, see
[`docs/field-contract.md`](./docs/field-contract.md).

## v3.x Highlights

- **v3.1.0 adds multi-part demo merging.** Pass split GOTV parts
  (`…-p1.dem …-p2.dem`) to `cs2df export` and they are merged into one
  coherent v3 ZIP; tick timelines are reconciled and duplicate round events
  from the recording resume are filtered automatically.
- **v3.0.4 fixes replay bomb-carrier state.** `flags & 2` now comes from the
  sampled player inventory, including rounds where C4 is assigned without a
  pickup event.
- **v3.0.3 preserves post-round tails.** Event/replay windows for non-final
  rounds now run until the next round starts, while `rounds.endTick` remains the
  result-decision tick.
- **v3.0.2 adds held utility state.** `player-economies.json` can carry
  freeze-time grenade inventory, and `replay.json` can carry per-frame held
  grenades so replay UIs can show utility after throws or drops.
- **`playerIndex` is the canonical player reference.** `steamId64` appears only
  in `players.json`; every other file references the row index.
- **Team and side are derived, not repeated.** Event rows no longer carry
  `teamKey` or `side`; consumers derive them from `players` + `rounds`.
- **`replay.json` is the unified state stream.** The old `positions-1s.json`
  path is gone; replay now carries position, view angles, HP, armor, money,
  equipment value, active weapon, place name, flash, and flags.
- **Columnar streams are integer-only.** Positions, angles, money, and equipment
  values are stored as compact integer arrays; high-frequency streams use delta
  encoding where appropriate.
- **`duels.json` is available for research exports.** It stores full-tick combat
  windows around kill/damage anchors for reaction-time and duel analysis.
- **The reference exporter is part of this repository.** `cs2df` exports,
  validates, batch-processes demos, and writes per-demo performance reports.

## Quick Start: Export a Demo

The reference exporter is in [`python/`](./python/) and uses
[`demoparser2`](https://github.com/LaihoE/demoparser) under the hood.

```bash
cd python
uv sync

# Standard export: required files + shots.json + replay.json
uv run cs2df export match.dem

# Multi-part GOTV recording (HLTV split demos) — merged into one ZIP
uv run cs2df export match-p1.dem match-p2.dem

# Research export: also includes full-tick duels.json windows
uv run cs2df export match.dem --research

# Validate the result
uv run cs2df validate match.zip --strict
```

Batch export a directory of demos:

```bash
uv run cs2df export-batch ./demos --workers 8 --descriptive
```

`export-batch` writes one ZIP per `.dem` plus `report.json` with per-demo
duration, output size, compression level, throughput, and stage timings. The
default ZIP compression level is `3`, chosen as a practical speed/size balance;
use `--compress-level 6` or `--compress-level 9` when smaller ZIPs matter more
than export time.

## Quick Start: Consume a Package

TypeScript consumers can use the bundled strict parser:

```ts
import { parseDemoPackage, decodeDelta } from "cs2-demo-format/parser";
import { readFileSync } from "node:fs";

const pkg = await parseDemoPackage(readFileSync("match.zip"));

console.log(pkg.manifest.schemaVersion);
console.log(pkg.files.match.mapName);
console.log(pkg.files.players[pkg.files.playerStats[0].playerIndex].name);

const firstReplayRound = pkg.files.replay?.rounds[0];
const firstPlayerTrack = firstReplayRound?.players[0];
const decodedX = firstPlayerTrack ? decodeDelta(firstPlayerTrack.x) : [];
```

Non-TypeScript consumers should validate against [`spec/`](./spec/), which is
generated from the same canonical Zod schemas.

## Example Export

[`fixtures/v3-mid/`](./fixtures/v3-mid/) is a checked-in v3 research fixture:

- Map: `de_anubis`
- Formal rounds: `21`
- Profile: `--research`
- Files: all required files plus `shots.json`, `replay.json`, and `duels.json`
- Schema version: `cs2-demo-format/3.0`

This fixture is intentionally useful as a first inspection target:

```bash
pnpm validate:fixtures
cat fixtures/v3-mid/manifest.json
cat fixtures/v3-mid/match.json
```

The largest files are the columnar streams (`replay.json` and `duels.json`),
which show the real shape and scale of a v3 export.

## Contract Guarantees

A conformant v3 package guarantees:

- `schemaVersion` is exactly `"cs2-demo-format/3.0"`.
- `players.json` row order is normative; that row index is `playerIndex`.
- `roundNumber` starts at `1` and only covers formal rounds.
- Event `roundNumber` values exist in `rounds.json`.
- Required files and required fields are present.
- JSON contains no bare `NaN`, `Infinity`, or `-Infinity`.
- Columnar arrays in a track have matching lengths.
- Delta-encoded arrays decode by prefix sum.
- Package-level QA checks catch common cross-file mismatches.

For damage/ADR, economy classification, KAST, clutch, replay, and duel-window
semantics, use [`docs/field-contract.md`](./docs/field-contract.md) as the
source of truth.

## Repository Map

| Path | Purpose |
|---|---|
| [`schemas/index.ts`](./schemas/index.ts) | Canonical Zod schemas, TypeScript types, and decode helpers. |
| [`spec/`](./spec/) | Generated JSON Schema files for non-TypeScript consumers. |
| [`parser/index.ts`](./parser/index.ts) | Reference TypeScript ZIP parser and schema validator. |
| [`python/`](./python/) | Reference Python exporter and validator CLI (`cs2df`). |
| [`tools/validate.py`](./tools/validate.py) | Thin wrapper around `cs2df validate`. |
| [`docs/field-contract.md`](./docs/field-contract.md) | File-by-file field semantics and calculation rules. |
| [`fixtures/`](./fixtures/) | Golden v3 fixture plus legacy history fixture. |
| [`CHANGELOG.md`](./CHANGELOG.md) | Release history. |

After any schema change, run:

```bash
pnpm check:versions
pnpm gen:schema
pnpm typecheck
pnpm validate:fixtures
```

## Known Implementations

- **Reference producer:** [`python/cs2df`](./python/)
- **Producer / toolkit:** [`cs2-demo-analysis-kit`](https://github.com/Starfie1d1272/cs2-demo-analysis-kit)
- **Consumer:** [`RivalHub`](https://github.com/Starfie1d1272/RivalHub)
- **Original event-extraction provenance:**
  [`DrEAmSs59/CS2-insight-agent`](https://github.com/DrEAmSs59/CS2-insight-agent),
  ported with the author's permission.

## Versioning

This package follows [Semantic Versioning](https://semver.org/).

- **Major:** required files, required fields, field removals, or semantic changes.
- **Minor:** additive optional files, optional fields, or generated schemas.
- **Patch:** documentation, validation tooling, or exporter fixes that do not
  change the ZIP contract.

Release tags use `vX.Y.Z` format.
Before tagging, run `pnpm check:versions` so `package.json`,
`python/pyproject.toml`, and `cs2df.__version__` stay aligned.

## License

MIT
