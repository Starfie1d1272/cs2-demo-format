# AGENTS.md

Repository guidance for Codex/Claude agents working on `cs2-demo-format`.

## Commands

```bash
pnpm typecheck              # tsc --noEmit — covers schemas/, parser/, scripts/
pnpm check:versions         # ensure npm package, Python package, and cs2df agree
pnpm gen:schema             # regenerate spec/*.schema.json from Zod schemas
pnpm validate:fixtures      # strict v3 fixture validation; legacy fixtures skipped

cd python && uv sync        # set up Python reference exporter
uv run cs2df export <dem>   # export a .dem to v3 ZIP
uv run cs2df validate <zip> # validate any v3 ZIP

python3 tools/validate.py export.zip  # thin wrapper (→ cs2df validate)
```

## Architecture

| Layer | Path | Purpose |
|---|---|---|
| Strict schema source | `schemas/index.ts` | Zod definitions for all ZIP files |
| Generated contract | `spec/*.schema.json` | JSON Schema for Python/Go/etc. consumers |
| Reference parser | `parser/index.ts` | Strict ZIP parser and schema validator |
| Reference exporter | `python/src/cs2df/` | Vectorized Python exporter CLI (demoparser2 → v3 ZIP) |
| Human contract | `docs/field-contract.md` | File-by-file field semantics and calculation rules |
| Validators | `scripts/validate-fixtures.ts`, `tools/validate.py` | Schema and package-level QA |

`schemas/index.ts` is the single source of truth for machine validation. After
any schema change, run `pnpm gen:schema` and commit the updated `spec/` files.

## v3 Contract

- Current package version: `3.0.3`.
- Current manifest version: `schemaVersion: "cs2-demo-format/3.0"`.
- Player references: `playerIndex` (zero-based index into players.json) replaces
  `steamId64` in all event/aggregate files. steamId64 appears only in players.json.
- Team/side fields removed from event rows: derived from `players[playerIndex].teamKey`
  + `rounds[roundNumber].teamASide/teamBSide`.
- `positions-1s.json` merged into `replay.json` (unified 8 Hz columnar stream).
- Delta encoding on position/angle/money arrays in columnar streams.
- Integer-only streams: no floats in replay/duels/shots; angles in `degrees × 10`.
- `kast_rounds` → `kastRounds`.
- Removed: `damages.victimHealthAfter`, `damages.victimArmorBefore`, `bombs.siteId`.
- Economy: `conversion` enum value removed; pistol-conversion rounds output `"full"`.

## Damage And ADR

```text
healthDamage = min(healthDamageRaw, victimHealthBefore)
playerStats.damageHealth = sum(damages.healthDamage for valid enemy damage)
playerStats.adr = damageHealth / rounds
```

## Versioning

- New required files, stricter required fields, field removals, or semantic
  changes → major.
- New optional fields or new generated schemas → minor.
- Documentation-only fixes → patch.
- For releases, update `package.json`, `python/pyproject.toml`,
  `python/src/cs2df/__init__.py`, `schemas/index.ts` header, `README.md`,
  `CHANGELOG.md`, regenerate `spec/` if schemas changed, run
  `pnpm check:versions`, verify, then tag `vX.Y.Z`.

## Notes

- `fixtures/de_ancient-2026-05-17/` is a legacy v1 fixture, skipped by validator.
- `fixtures/v3-mid/` is the v3 golden fixture (de_anubis, 21 rounds, research profile).
- Do not commit `.omc/`, `.DS_Store`, `__pycache__/`, or `node_modules/`.
