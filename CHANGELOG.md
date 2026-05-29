# Changelog

All notable changes to cs2-demo-format are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) · [Semver](https://semver.org/)

## [Unreleased]

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
