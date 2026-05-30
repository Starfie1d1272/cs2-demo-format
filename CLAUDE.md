# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
pnpm typecheck         # tsc --noEmit — covers schemas/, parser/, scripts/
pnpm gen:schema        # regenerate spec/*.schema.json from Zod schemas (commit the output)
pnpm validate:fixtures # validate all fixtures against SCHEMAS_BY_KEY
```

## Architecture

This repository is the **canonical format contract** between `cs2-insight-agent` (Python producer)
and `RivalHub` (Node.js consumer). It contains three layers:

| Layer | Path | Purpose |
|---|---|---|
| Schema (true value) | `schemas/index.ts` | Zod definitions for all 14 file types in the ZIP |
| Reference parser | `parser/index.ts` | `parseDemoPackage(buffer)` — reads ZIP, validates, sanitizes, filters warmup |
| Language-neutral contract | `spec/*.schema.json` | Generated JSON Schema; Python/Go/etc. consume these directly |

**`schemas/index.ts` is the single source of truth.** All other outputs (`spec/*.schema.json`,
TypeScript types) are derived from it. After any schema change, run `pnpm gen:schema` and commit
the updated `spec/` files.

### Key exports

- `SCHEMAS_BY_KEY` — `manifest.files` key → Zod schema map (used by parser dispatch)
- `FILE_SCHEMAS` — deprecated alias for `SCHEMAS_BY_KEY`
- `parseDemoPackage(buffer)` — from `cs2-demo-format/parser`

### Validation strictness

Most fields use `nullInt`/`nullReal`/`nullBool` = `z.number().nullable().optional()` — meaning
a field can be present-with-value, present-as-null, or absent entirely. This is intentional:
null means "not tracked / didn't happen this map", and many fields are legitimately absent in
older exporter versions.

The only hard constraints are:
- **Identity fields**: `steamId64`, `roundNumber` are non-null required
- **Enum values**: `side` (`t|ct|unknown`), `economyTypeSchema` (`pistol|eco|semi|force|full`)
- **No unknown fields**: `zod-to-json-schema` emits `additionalProperties: false` in `spec/*.schema.json` — extra fields are rejected

`vec3Schema` coords are `nullable` — the exporter emits `NaN` for unavailable positions;
the parser sanitizes these to `null` before schema validation.

### Important field notes

- `playerStatsRowSchema`: `bombPlantCount` / `bombDefuseCount` (no `-ed` suffix) matches actual
  exporter output. `kast_rounds` is the raw KAST round count (integer companion to `kast` %).
- `economyTypeSchema` values: `pistol | eco | semi | force | full`. Note: older code used
  `full_buy` — that is incorrect; the actual data uses `full`.
- `match.json` is a single object (not array); `parseDemoPackage` wraps it in `[...]` for
  uniform access via `parsed.files.match[0]`.

## Key domain rules

**Economy classification** (`economyTypeSchema`) — priority order, first match wins:
1. `pistol` — first round of each half (round number, NOT equipment/money values)
2. `full` — `equipmentValue >= 4000`
3. `eco` — `moneySpent < 1000` AND `equipmentValue < 2000`
4. `force` — `startMoney > 0` AND `moneySpent / startMoney > 0.75`
5. `semi` — everything else

Team-level economy (`teamEconomySchema`) uses 5-player majority vote; ties resolve conservatively
(`eco < semi < force < full`). Currently `null` in exporter output.

## Versioning conventions

- New required files or field removals → **major** bump
- New optional fields, new file schemas, field name corrections → **minor** bump
- Bug fixes to JSDoc / docs only → **patch** bump
- Update `package.json` version, `schemas/index.ts` header comment, `CHANGELOG.md`, then tag `vX.Y.Z`
- After any schema change: `pnpm gen:schema` → commit updated `spec/*.schema.json`
