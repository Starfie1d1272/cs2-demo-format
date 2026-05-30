# AGENTS.md

Repository guidance for Codex/Claude agents working on `cs2-demo-format`.

## Commands

```bash
pnpm typecheck         # tsc --noEmit — covers schemas/, parser/, scripts/
pnpm gen:schema        # regenerate spec/*.schema.json from Zod schemas
pnpm validate:fixtures # strict v2 fixture validation; legacy v1 fixtures are skipped
python3 tools/validate.py export.zip
```

If pnpm is blocked by local approve-builds policy, use the checked out binaries directly:

```bash
./node_modules/.bin/tsc --noEmit
./node_modules/.bin/tsx scripts/gen-json-schema.ts
./node_modules/.bin/tsx scripts/validate-fixtures.ts
```

## Architecture

This repository is an implementation-neutral ZIP data contract for parsed CS2
demo exports. Current known implementations include `DrEAmSs59/CS2-insight-agent`
as a producer and `RivalHub` as a consumer, but the format should stay reusable
for other producers, importers, validators, and analysis tools.

| Layer | Path | Purpose |
|---|---|---|
| Strict schema source | `schemas/index.ts` | Zod definitions for all ZIP files |
| Generated contract | `spec/*.schema.json` | JSON Schema for Python/Go/etc. consumers |
| Reference parser | `parser/index.ts` | Strict ZIP parser and schema validator |
| Human contract | `docs/field-contract.md` | File-by-file field semantics and calculation rules |
| Validators | `scripts/validate-fixtures.ts`, `tools/validate.py` | Schema and package-level QA |

`schemas/index.ts` is the single source of truth for machine validation. After
any schema change, run `pnpm gen:schema` and commit the updated `spec/` files.

## v2 Strict Contract

- Current package version: `2.0.0`.
- Current manifest version: `schemaVersion: "cs2-demo-format/2.0"`.
- Strict exports must not rely on consumers to sanitize or repair data.
- `side` is only `"t" | "ct"`; `"unknown"` in formal rounds is a producer error.
- `teamKey` is only `"teamA" | "teamB"`; real team names live in `match.teamA.name`
  and `match.teamB.name`, which may be `null` when the demo does not provide names.
- Formal `roundNumber` starts at 1 and must be continuous. Warmup / round 0 rows
  must not appear in event files.
- Tick fields are positive integers. Unknown tick values are producer errors.
- `NaN` / `Infinity` must never be emitted in JSON.

## Damage And ADR

`damages.healthDamageRaw` is the parser's raw uncapped damage.
`damages.healthDamage` is capped effective damage:

```text
healthDamage = min(healthDamageRaw, victimHealthBefore)
```

`playerStats.damageHealth` aggregates capped effective damage, and
`playerStats.adr = damageHealth / rounds`. Utility damage uses the same capped
effective damage basis.

## Versioning

- New required files, stricter required fields, field removals, or semantic
  changes → major.
- New optional fields or new generated schemas → minor.
- Documentation-only fixes → patch.
- For releases, update `package.json`, `schemas/index.ts` header, `README.md`,
  `CHANGELOG.md`, regenerate `spec/`, verify, then tag `vX.Y.Z`.

## Notes

- `fixtures/de_ancient-2026-05-17/` is currently a legacy v1 fixture. `pnpm
  validate:fixtures` skips legacy fixtures until a v2 golden fixture is generated.
- Do not commit local state directories or generated caches such as `.omc/`,
  `.DS_Store`, `__pycache__/`, or `node_modules/`.
