# Changelog

All notable changes to cs2-demo-format are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) · [Semver](https://semver.org/)

## [Unreleased]

### Changed
- Split player and team economy typing: `player-economies.json.type` remains
  `pistol/eco/semi/force/full`, while `rounds.json.teamAEconomy/teamBEconomy`
  additionally allow `conversion` for the team that won R1/R13 in the following round.
- Clarified economy classification: pistol rounds are R1/R13 only in MR12, OT is not
  a pistol round; eco uses the sub-$1000 save bucket and force uses `spent/start >= 0.80`.

## [2.2.0] - 2026-06-01

### Added
- Precision checks in `tools/validate.py`: float fields in vec3 objects (x/y ≤2dp, z ≤1dp),
  yaw/pitch (≤1dp), and `flashDurationRemaining` (≤1dp) are validated for parser precision
  noise. Violations are reported as errors.

### Changed
- `manifest.files.replay` and `replay` registered as an optional file key in the validator.

## [2.1.0] - 2026-06-01

### Added
- Optional `replay.json` stream and `manifest.files.replay` key: a compact, columnar,
  quantized player-movement format tuned for a 2D replay viewer. Per round, each player
  carries parallel integer arrays (`x`/`y`/`z`/`yaw`/`hp`/`weapon`/`flags`) of length
  `frameCount`; the tick of frame `i` is `startTick + i * tickStep`. Static identity
  (`steamId64`/`teamKey`/`side`) is stored once per player per round. Coordinates are
  integers in game units divided by `meta.coordScale`. `weapon` indexes a top-level
  `weaponDict`; `flags` is a per-frame bitfield (1=alive, 2=hasBomb, 4=hasDefuseKit, 8=flashed).

### Notes
- Backward compatible within format major `cs2-demo-format/2.0`: `replay.json` is optional,
  the `schemaVersion` literal is unchanged, and existing consumers ignore the new file.
  `positions-1s.json` is unchanged and remains the 1 Hz analytics/heatmap stream.

## [2.0.0] - 2026-05-31

### Changed
- Promoted the package to a strict export contract with `schemaVersion: "cs2-demo-format/2.0"`.
- Removed `"unknown"` from strict `side` values; formal round/player events must resolve to
  `"t"` or `"ct"`.
- Restricted `teamKey` to `"teamA" | "teamB"`; display names live in `match.teamA.name` and
  `match.teamB.name` and may be `null` when the demo does not provide names.
- Changed most schema fields from optional/nullable legacy columns into required strict fields.
- Made the reference parser strict: it no longer sanitizes `NaN` / `Infinity` or filters warmup
  rows after validation.
- Reworked ADR semantics: `damages.healthDamage` is capped effective damage used for ADR,
  while `damages.healthDamageRaw` stores the raw parser value.

### Added
- `docs/field-contract.md` — file-by-file strict ZIP field contract.
- Bilingual README entrypoints (`README.md` and `README.zh-CN.md`) with implementation-neutral
  positioning for producers, consumers, validators, and analysis tools.
- Formal kill duel / flash fields: `flashAssisterSteamId64`, `killerActiveWeapon`,
  `victimActiveWeapon`.
- Formal economy equipment fields: `hasArmor`, `hasHelmet`, `hasDefuseKit`, `primaryWeapon`,
  `secondaryWeapon`, `grenadeCount`.
- Formal grenade/bomb correlation fields: `grenadeId`, `destroyTick`, `siteId`.
- Formal stats fields: `flashAssistCount`, `enemyFlashDurationSeconds`,
  `teamFlashDurationSeconds`, `combatDeathCount`, `bombDeathCount`.
- Package-level QA checks in fixture and ZIP validators.
- `pnpm-workspace.yaml` build approval for `esbuild`, so pnpm validation can run without the
  local approve-builds prompt.
- `fixtures/README.md` documenting the legacy v1 fixture and v2 golden-fixture requirement.

## [1.3.0] - 2026-05-30

### Added
- `matchSchema` — contract for `match.json` (match-level summary: team names, final scores,
  map name, duration, server name, source). Match is a single object, not an array.
  Added `match` key to `SCHEMAS_BY_KEY`.
- `teamSummarySchema` — reusable sub-schema for teamA/teamB objects (`teamKey`, `name`, `score`).
- `parser/index.ts` — reference TypeScript parser (`parseDemoPackage(buffer)`) that reads a ZIP
  buffer, validates every file against `SCHEMAS_BY_KEY`, sanitizes NaN/Infinity → null, and
  filters warmup rows (roundNumber=0). Now a direct dependency of the package (`jszip`).
- `spec/*.schema.json` — 14 language-neutral JSON Schema files generated from the Zod schemas
  via `zod-to-json-schema`. Committed to the repo so non-TypeScript consumers (Python, etc.)
  can validate without any Node.js dependency.
- `fixtures/de_ancient-2026-05-17/` — golden-sample fixture from a real match. All required
  files (manifest, match, players, rounds, player-stats, player-economies, kills, damages,
  blinds, bombs, grenades, clutches) validate cleanly against `SCHEMAS_BY_KEY`.
- `scripts/gen-json-schema.ts` — regenerates `spec/*.schema.json` (`pnpm gen:schema`).
- `scripts/validate-fixtures.ts` — validates all fixtures against schemas (`pnpm validate:fixtures`).
- `FILE_SCHEMAS` re-export alias (`@deprecated`) pointing to `SCHEMAS_BY_KEY` for backward compat.

### Fixed
- `vec3Schema` — coordinate components are now `z.number().nullable()`. The exporter emits
  `NaN` for unavailable positions (e.g. spectator kills); after sanitization these become `null`.
- `playerStatsRowSchema` — corrected field names: `bombPlantCount` (was `bombPlantedCount`),
  `bombDefuseCount` (was `bombDefusedCount`). Added `kast_rounds` (raw KAST round count,
  complementary to the percentage `kast` field).

## [1.2.0] - 2026-05-30

### Added
- `economyTypeSchema` — restored `"pistol"` enum value for the first round of each half.
  `pistol` has priority 0 and is determined by round number (not equipment / money values);
  applies to round 1 and the opening round of the second half and any overtime halves.

## [1.1.0] - 2026-05-30

### Changed
- `economyTypeSchema` enum revised to `"eco" | "semi" | "force" | "full"`.
  Replaces the incorrect `"full_buy"` and `"pistol"` values from v1.0.0.
  - `full`  — `equipment_value >= 4000` (AK + armor + util baseline)
  - `eco`   — `money_spent < 1000` AND `equipment_value < 2000`
  - `force` — `start_money > 0` AND `money_spent / start_money > 0.75`
  - `semi`  — everything else (fallback)
  Priority is evaluated in the order listed above; the first matching rule wins.
- `teamEconomySchema` typed as `economyTypeSchema.nullable()` (was `z.unknown().nullable()`).
  Team classification uses 5-player majority vote; ties resolve conservatively
  (`eco < semi < force < full`). Field remains null in current exporter output.

### Added
- `README.md` — Economy classification algorithm documented with priority table and price
  reference. `teamAEconomy / teamBEconomy` section updated (removes [TBD] status).
- `tsconfig.json` — Added missing TypeScript configuration so `pnpm typecheck` works.

## [1.0.0] - 2026-05-29

### Changed
- `schemaVersion` string renamed from `"rivalhub-demo-export/1"` to `"cs2-demo-format/1.0"`.
  Consumers should accept both strings during transition.
- `economyTypeSchema` corrected to `"eco" | "force" | "full_buy" | "pistol"` (was `"semi"/"full"`
  which did not match real exporter output).
- `playerEconomyRowSchema.type` upgraded from `nullStr` to the proper `economyTypeSchema` enum.
- `teamEconomySchema` changed from `z.unknown()` to `z.unknown().nullable()` — removes TBD
  status; field is confirmed null in current exporter and reserved for future use.

### Added
- `playerStatsRowSchema.rounds` — total map rounds (added in cs2-insight-agent v2.1.2+;
  `null` in older exports). Enables accurate per-round rate computation without fallback.
- `grenadeRowSchema` JSDoc note: `throwPosition` is `{0,0,0}` for most grenades (exporter
  limitation); use `effectPosition` for detonation point.
- `clutchRowSchema.survived` and `killCount` fields (were present in exporter output but
  missing from schema; now explicit).

## [0.1.0] - 2026-05-29

### Added
- Initial pre-release framework derived from RivalHub v1.26.x production schema
- `schemas/index.ts` — Zod schemas for all 13 file types (manifest, players, rounds,
  player-stats, player-economies, kills, damages, blinds, bombs, clutches, grenades,
  shots, positions-1s)
- `docs/zip-structure.md` — ZIP layout and file-by-file field reference
- `fixtures/` — placeholder directory for sample ZIP fixtures (to be added at v1.0.0)

### Known TBD
- `teamAEconomy` / `teamBEconomy` fields in `rounds.json` are typed as `z.unknown()`
  pending confirmation from cs2-insight-agent v1 first export
- `schemaVersion` literal (`"rivalhub-demo-export/1"`) will be renamed to
  `"cs2-demo-format/1.0"` at v1.0.0
