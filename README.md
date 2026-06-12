# cs2-demo-format

**Language:** English | [简体中文](./README.zh-CN.md)

`cs2-demo-format` is an implementation-neutral data contract for parsed
Counter-Strike 2 demo exports. It defines a strict ZIP package layout,
machine-readable schemas, and validation rules that producers, consumers, and
analysis tools can share without coupling to one application.

The format is intended to be useful beyond a single importer/exporter pair: a
parser can produce it, a web app can ingest it, a rating engine can score it,
and a standalone analysis tool can validate and query it. The contract lives in
`schemas/index.ts` + `spec/*.schema.json`; any producer that emits valid
packages is conformant.

Current known implementations:

- **Producer**: [`cs2df`](./python/) (reference CLI bundled in this repo),
  [`cs2-demo-analysis-kit`](https://github.com/Starfie1d1272/cs2-demo-analysis-kit)
- **Consumer**: [`RivalHub`](https://github.com/Starfie1d1272/RivalHub)
- Original provenance: event-extraction logic traces back to
  [`DrEAmSs59/CS2-insight-agent`](https://github.com/DrEAmSs59/CS2-insight-agent),
  ported with the author's permission.

## What This Repo Contains

| Path | Purpose |
|---|---|
| [`schemas/index.ts`](./schemas/index.ts) | Canonical Zod schemas + TypeScript types. Single source of truth. |
| [`spec/*.schema.json`](./spec/) | Generated JSON Schema files for Python, Go, Rust, and other non-TS tools. |
| [`parser/index.ts`](./parser/index.ts) | Reference TypeScript ZIP parser. Validates every file against the schemas. |
| [`python/`](./python/) | Reference Python exporter CLI (`cs2df export` / `cs2df validate`). |
| [`tools/validate.py`](./tools/validate.py) | Thin Python wrapper (`→ cs2df validate`). |
| [`docs/field-contract.md`](./docs/field-contract.md) | File-by-file field semantics, calculation rules, and v2→v3 migration. |
| [`fixtures/`](./fixtures/) | Golden fixtures (`fixtures/v3-mid/` — de_anubis, 21 rounds, research profile). |

`schemas/index.ts` is authoritative. After any schema change, run
`pnpm gen:schema` and commit the updated `spec/` files.

## ZIP Package Structure (v3)

A v3 export is a ZIP file with a `manifest.json` plus the data files it declares.

| File | Required | Shape | Purpose |
|---|---:|---|---|
| `manifest.json` | Yes | object | Package metadata, schema version (`"cs2-demo-format/3.0"`), demo identity, file index. |
| `match.json` | Yes | object | Match summary: map, tickrate, team slots, scores, duration. |
| `players.json` | Yes | array | Player identities + `teamKey`. **Row order is normative** — the index is the `playerIndex` used across every other file. |
| `rounds.json` | Yes | array | Formal round timeline, sides, score state, team economy (majority vote), winner, end reason. |
| `player-stats.json` | Yes | array | Per-player aggregate stats (kills, ADR, KAST, multikills, clutches, flashes…). |
| `player-economies.json` | Yes | array | Per-player per-round economy snapshot (money, spend, equipment, buy type). One row per player per round. |
| `kills.json` | Yes | array | Kill events: participants via `playerIndex`, weapons, positions, trade/flash/smoke flags. |
| `damages.json` | Yes | array | Damage events: raw + capped effective health damage, armor damage, hitgroup, positions. |
| `blinds.json` | Yes | array | Flash blind events: flasher, victim, duration, flashId linkage. |
| `bombs.json` | Yes | array | Bomb lifecycle: `plant_begin`, `planted`, `defuse_begin`, `defused`, `exploded`, `dropped`, `picked_up`. |
| `grenades.json` | Yes | array | Grenade throw/detonation events: thrower, positions, timings, destroy tick. |
| `clutches.json` | Yes | array | Derived 1vN clutch situations: clutcher, opponent count, won/survived/killCount. |
| `shots.json` | No | columnar | Weapon-fire tracks grouped by (round, playerIndex), delta-encoded. |
| `replay.json` | No | columnar | Unified 8 Hz player-state stream — positions, angles, HP, armor, money, equipment, weapon, place name, flash, flags. Merges the former `positions-1s.json`. |
| `duels.json` | No | columnar | Full-tick combat-window stream for reaction-time research. Optional, emitted with `--research`. |

## What Changed in v3

- **`playerIndex` replaces `steamId64`** in every file except `players.json`.
  SteamID64 strings appear exactly once per player.
- **`teamKey` / `side` removed from event rows** — derived from
  `players[playerIndex].teamKey` + `rounds[roundNumber]`.
- **`positions-1s.json` merged into `replay.json`** — one unified 8 Hz
  columnar stream with all former positions-1s fields (pitch, armor, money,
  equipValue, flash, place name, flags).
- **Delta encoding** on position / angle / money arrays in all columnar
  streams. Decode with a running prefix sum (`decodeDelta()` helper exported).
- **Integer-only columns** — no floats in `replay.json`, `duels.json`, or
  `shots.json`. Angles in `degrees × angleScale` (default 0.1°), flash in
  tenths of a second, positions in integer game units.
- **Field cleanup** — `kast_rounds` → `kastRounds`; redundant
  `damages.victimHealthAfter` / `victimArmorBefore` removed; `bombs.siteId`
  removed.
- **`duels.json`** (research profile) — merged combat windows at native
  tickrate around kill/damage anchors, for reaction-time measurement.
- **`shots.json`** restructured from row-oriented to columnar tracks.
- Full v2→v3 migration table in [`docs/field-contract.md`](./docs/field-contract.md).

## Foundational Rules (Apply to All Versions)

These rules are not version-specific — they are the baseline contract:

- `roundNumber` starts at `1` and increments continuously. Warmup / round-0
  rows must never appear in event or aggregate files.
- Tick fields are positive integers. A missing tick is a producer error, not
  `0` or `null`.
- JSON must not contain bare `NaN`, `Infinity`, or `-Infinity`.
- `null` is reserved for values the demo may genuinely not provide (e.g. team
  display name, demo hash). It is not a fallback for parser errors.
- Required files and required fields are required because they are not
  demo-optional. Missing values indicate producer failure.

## Damage & ADR Semantics

CS2 parsers may expose raw damage larger than the victim's remaining HP. Rating
systems and platform-style ADR use effective damage capped by remaining HP:

```text
damages.healthDamage    = min(healthDamageRaw, victimHealthBefore)
playerStats.damageHealth = Σ damages.healthDamage  (anti-enemy only)
playerStats.adr          = damageHealth / rounds
```

Utility damage (HE, molotov, incendiary) uses the same capped effective damage
basis.

## Quick Start

### Export a demo (Python)

```bash
cd python && uv sync                     # one-time setup
uv run cs2df export match.dem            # → match.zip (standard 8 Hz replay)
uv run cs2df export match.dem --research # + duels.json (full-tick combat windows)
```

### Consume a package (TypeScript)

```ts
import { type PlayerStatsRow } from "cs2-demo-format";
import { parseDemoPackage, decodeDelta } from "cs2-demo-format/parser";
import { readFileSync } from "node:fs";

const pkg = await parseDemoPackage(readFileSync("match.zip"));
console.log(pkg.manifest.mapName, pkg.files.playerStats.length);
```

Non-TypeScript consumers should use the JSON Schema files in [`spec/`](./spec/).

## Validation

```bash
# Repo-level checks
pnpm typecheck
pnpm gen:schema
pnpm validate:fixtures

# Validate any exported ZIP
uv run cs2df validate export.zip
python3 tools/validate.py export.zip   # thin wrapper
```

## Producer Requirements

A conformant v3 producer must:

- Emit all required files and `schemaVersion: "cs2-demo-format/3.0"`.
- Write `players.json` in a stable order (recommended: teamKey then steamId64).
- Use that same order as `playerIndex` in every other file.
- Filter warmup / non-formal rounds before writing.
- Produce continuous `roundNumber` starting at `1`.
- Ensure every event `roundNumber` exists in `rounds.json`.
- Emit exactly `rounds.length × players.length` rows in `player-economies.json`.
- Ensure `playerStats.rounds == rounds.length`.
- Compute ADR, KAST, first-kill/multikill/clutch counts from the event files
  using the rules in [`docs/field-contract.md`](./docs/field-contract.md).
- Delta-encode position / angle / money arrays in columnar streams.
- Fail the export instead of writing unknown sides, zero ticks, missing
  participants, or non-finite numbers.

## Versioning

This package follows [Semantic Versioning](https://semver.org/).

- **Major** — new required files, required field changes, field removals,
  semantic changes.
- **Minor** — new optional fields / files, additive schemas.
- **Patch** — documentation fixes, tooling fixes.

Release tags use `vX.Y.Z` format (e.g. `v3.0.0`).

## Docs

- Full field contract: [`docs/field-contract.md`](./docs/field-contract.md)
- JSON Schema: [`spec/`](./spec/)
- Reference exporter: [`python/`](./python/)
- Fixtures: [`fixtures/`](./fixtures/)
- Release history: [`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT
