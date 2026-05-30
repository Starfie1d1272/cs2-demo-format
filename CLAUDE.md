# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
pnpm typecheck   # tsc --noEmit — only CI gate in this repo
```

No build step, no test runner, no linter. The sole quality gate is TypeScript compilation.

## Architecture

This is a **pure specification repository** — no runtime logic, no application code. It defines the ZIP-based export format used between cs2-insight-agent (producer) and RivalHub (consumer).

The entire schema lives in one file: [`schemas/index.ts`](./schemas/index.ts). It exports:
- Zod schemas (named `*Schema`) for each of the 13 JSON files in the ZIP
- Corresponding TypeScript types (named with the `Row` / `Manifest` suffix via `z.infer<>`)
- `SCHEMAS_BY_KEY` — a map from manifest file key to schema, used for validation dispatch

`README.md` is the human-readable field reference (the canonical doc for non-TypeScript consumers). Keep it in sync with `schemas/index.ts` whenever schema or semantics change.

## Key domain rules

**Economy classification** (`economyTypeSchema`) — priority order, first match wins:
1. `full` — `equipmentValue >= 4000`
2. `eco` — `moneySpent < 1000` AND `equipmentValue < 2000`
3. `force` — `startMoney > 0` AND `moneySpent / startMoney > 0.75`
4. `semi` — everything else

Team-level economy (`teamEconomySchema`) uses 5-player majority vote; ties resolve conservatively (`eco < semi < force < full`). Currently `null` in exporter output.

## Versioning conventions

- Enum value changes or field removals → **minor** bump (breaking for consumers)
- New optional fields → **minor** bump
- Bug fixes to JSDoc / docs only → **patch** bump
- Update both `package.json` version and the header comment in `schemas/index.ts`, then tag `vX.Y.Z`
