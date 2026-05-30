# cs2-demo-format

**Language:** English | [简体中文](./README.zh-CN.md)

`cs2-demo-format` is an implementation-neutral data contract for parsed Counter-Strike 2 demo exports.
It defines a strict ZIP package layout, machine-readable schemas, and validation rules that producers,
consumers, and analysis tools can share without coupling to one application.

The format is intended to be useful beyond a single importer/exporter pair: a parser can produce it,
a web app can ingest it, a rating engine can score it, and a standalone analysis tool can validate and
query it.

Current known implementations:

- Producer: [`DrEAmSs59/CS2-insight-agent`](https://github.com/DrEAmSs59/CS2-insight-agent)
- Consumer: [`Starfie1d1272/RivalHub`](https://github.com/Starfie1d1272/RivalHub)

## What This Package Contains

| Path | Purpose |
|---|---|
| [`schemas/index.ts`](./schemas/index.ts) | Canonical Zod schemas and TypeScript types. This is the single source of truth. |
| [`spec/*.schema.json`](./spec/) | Generated JSON Schema files for Python, Go, Rust, and other non-TypeScript tools. |
| [`parser/index.ts`](./parser/index.ts) | Reference ZIP parser that validates package contents against the schemas. |
| [`tools/validate.py`](./tools/validate.py) | Python validator for checking exported ZIP packages outside Node.js. |
| [`docs/field-contract.md`](./docs/field-contract.md) | File-by-file strict field contract: semantics, calculation rules, ranges, and nullable rules. |
| [`fixtures/`](./fixtures/) | Real-world fixture area. Current checked-in fixture is legacy v1 and is skipped by strict v2 validation. |

`schemas/index.ts` is authoritative. When schemas change, regenerate `spec/` and commit the generated JSON Schema files.

## ZIP Export Structure

A v2 export is a ZIP file with a `manifest.json` plus the files declared in `manifest.files`.
All required files must be present. Optional files may be omitted from `manifest.files`.

| File | Required | Shape | Purpose |
|---|---:|---|---|
| `manifest.json` | Yes | object | Package metadata, schema version, source demo identity, and file index. |
| `match.json` | Yes | object | Match summary: map, tickrate, team slots, scores, duration, and source. |
| `players.json` | Yes | array | Player identities and stable `teamKey` assignment. |
| `rounds.json` | Yes | array | Formal round timeline, sides, score state, economy, winner, and reason. |
| `player-stats.json` | Yes | array | Per-player full-map aggregate stats derived from formal rounds only. |
| `player-economies.json` | Yes | array | Per-player per-round money, spend, equipment value, inventory, and buy type. |
| `kills.json` | Yes | array | Kill events with participants, sides, weapons, positions, trade flags, and duel context. |
| `damages.json` | Yes | array | Damage events with raw and effective health damage, armor damage, victim health, and positions. |
| `blinds.json` | Yes | array | Flash blind events with thrower, victim, assister linkage, duration, and positions. |
| `bombs.json` | Yes | array | Bomb planted, defused, exploded, and dropped events. |
| `grenades.json` | Yes | array | Grenade lifecycle events and positions. |
| `clutches.json` | Yes | array | Derived clutch situations tied back to rounds and players. |
| `shots.json` | No | array | Shot events. Optional because this can be high volume. |
| `positions-1s.json` | No | array | One-second player state snapshots. Optional because this can be high volume. |

## Strict v2 Contract

Version 2 is intentionally strict. The export should be valid when it is written; consumers should not
need to repair malformed data.

- `manifest.schemaVersion` must be `"cs2-demo-format/2.0"`.
- `roundNumber` identifies formal rounds only. It starts at `1`, increments by `1`, and must be continuous.
- Warmup rows and round `0` rows must not appear in event or aggregate files.
- Tick fields are positive integers. Unknown ticks are export failures, not `0` or `null`.
- JSON must not contain bare `NaN`, `Infinity`, or `-Infinity`.
- `teamKey` is an internal stable slot: `"teamA"` or `"teamB"`. Real names live in `match.teamA.name` and `match.teamB.name`; they may be `null` when the demo does not provide names.
- `side` is `"t"` or `"ct"` in formal rounds. `"unknown"` is not valid v2 data.
- Required files and required fields are required because they are not demo-optional. Missing values indicate producer failure unless the field contract explicitly allows `null`.
- `null` is reserved for values that the demo may genuinely not provide, such as team display names or source hash. It is not a fallback for parser errors.

## Damage And ADR Semantics

CS2 parsers may expose raw damage that is larger than the victim's remaining HP. Rating systems and
platform-style ADR usually need effective damage capped by remaining HP.

v2 stores both values:

```text
damages.healthDamageRaw = parser raw uncapped health damage
damages.healthDamage    = min(healthDamageRaw, victimHealthBefore)
```

Aggregate stats use effective damage:

```text
playerStats.damageHealth = sum(damages.healthDamage for valid enemy damage)
playerStats.adr          = playerStats.damageHealth / playerStats.rounds
playerStats.utilityAdr   = playerStats.utilityDamage / playerStats.rounds
```

The full inclusion and exclusion rules for self damage, team damage, world damage, bomb damage,
KAST, first kills, assists, clutches, and utility damage are documented in
[`docs/field-contract.md`](./docs/field-contract.md).

## Producer Requirements

A producer should treat this package as an export contract, not as a loose log dump.

At minimum, a v2 producer must:

- emit all required files listed in the ZIP structure;
- emit `schemaVersion: "cs2-demo-format/2.0"`;
- filter warmup and non-formal rows before writing;
- produce continuous formal rounds starting at `1`;
- ensure every event `roundNumber` exists in `rounds.json`;
- ensure every SteamID, team key, and side can be reconciled with `players.json` and the round side mapping;
- include one `player-economies.json` row for every player in every formal round;
- ensure `playerStats.rounds` equals the number of formal rounds;
- compute `adr`, `kast`, `utilityAdr`, first-kill counts, multikill counts, and clutch counts from the base event files using the documented rules;
- fail the export instead of writing unknown sides, zero ticks, missing required participants, or non-finite numbers.

Derived analytical concepts that require product-specific interpretation, such as round swing or custom
rating weights, should stay outside the base format. The format should preserve enough raw ingredients
for those tools to compute their own metrics.

## Consumer Usage

TypeScript consumers can import the schemas and reference parser directly:

```ts
import { SCHEMAS_BY_KEY, type PlayerStatsRow } from "cs2-demo-format";
import { parseDemoPackage } from "cs2-demo-format/parser";
import { readFileSync } from "node:fs";

const parsed = await parseDemoPackage(readFileSync("match-export.zip"));
const stats: PlayerStatsRow[] = parsed.files.playerStats;

console.log(parsed.manifest.mapName, stats.length);
```

Non-TypeScript consumers should use the generated JSON Schema files in [`spec/`](./spec/).

## Validation

Run the repository checks:

```bash
pnpm typecheck
pnpm gen:schema
pnpm validate:fixtures
```

Validate a specific ZIP export:

```bash
python3 tools/validate.py match-export.zip
```

If local pnpm policy blocks dependency build scripts, approve the required builds once:

```bash
pnpm approve-builds
```

The validator performs both schema checks and package-level QA, including round continuity, missing
event rounds, economy coverage, aggregate stat alignment, invalid ticks, unresolved SteamIDs, unresolved
team/side mappings, and damage/ADR consistency.

## Versioning

This package follows Semantic Versioning.

- Major: new required files, required field changes, field removals, or semantic changes.
- Minor: new optional fields, new optional files, additive schemas, or non-breaking validation improvements.
- Patch: documentation fixes, generated-schema corrections that do not change the contract, and tooling fixes.

Release tags use the `vX.Y.Z` format, for example `v2.0.0`.

## Documentation Map

- Full field contract: [`docs/field-contract.md`](./docs/field-contract.md)
- JSON Schema output: [`spec/`](./spec/)
- Fixture notes: [`fixtures/README.md`](./fixtures/README.md)
- Release history: [`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT
