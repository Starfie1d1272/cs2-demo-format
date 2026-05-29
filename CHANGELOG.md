# Changelog

All notable changes to cs2-demo-format are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) · [Semver](https://semver.org/)

## [Unreleased]

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
