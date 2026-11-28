# AGENTS.md

Repository guidance for Codex/Claude agents working on `cs2-demo-format`.

## Commands

```bash
pnpm typecheck              # tsc --noEmit — covers schemas/, parser/, scripts/
pnpm check:versions         # ensure package.json, crates/cs2df/Cargo.toml, and schemas/index.ts agree
pnpm gen:schema             # regenerate spec/*.schema.json from Zod schemas
pnpm validate:fixtures      # strict v3 fixture validation; legacy fixtures skipped
pnpm test:parser            # verify 3.0, 3.1, and 3.x forward-compatible TS parsing

cargo test --workspace      # run Rust tests (cs2df + demoparser)
cargo build --release -p cs2df
./target/release/cs2df export <dem>       # export a .dem to v3.1 ZIP
./target/release/cs2df validate <zip>     # validate any 3.0 / 3.1 ZIP
```

## Architecture

| Layer | Path | Purpose |
|---|---|---|
| Strict schema source | `schemas/index.ts` | Zod definitions for all ZIP files |
| Generated contract | `spec/*.schema.json` | JSON Schema for non-TypeScript consumers |
| Reference parser | `parser/index.ts` | 3.x forward-compatible ZIP parser |
| Reference exporter | `crates/cs2df/` | Pure-Rust exporter & validator CLI (`cs2df`) |
| Human contract | `docs/field-contract.md` | File-by-file field semantics and calculation rules |
| Validators | `scripts/validate-fixtures.ts`, `scripts/test-parser.ts` | Schema, forward-compat, and package-level QA |

`schemas/index.ts` is the single source of truth for machine validation. After
any schema change, run `pnpm gen:schema` and commit the updated `spec/` files.

## v3 Contract

- Current package version: tracked in `package.json`, `crates/cs2df/Cargo.toml`, and `schemas/index.ts` header (verified via `pnpm check:versions`).
- Current manifest version: `schemaVersion: "cs2-demo-format/3.1"` (validators accept `3.0` and `3.1`; TS consumer accepts `3.x`).
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
- For releases, update `package.json`, `crates/cs2df/Cargo.toml`,
  `schemas/index.ts` header, `README.md`, `README.zh-CN.md`,
  `CHANGELOG.md`, regenerate `spec/` if schemas changed, run
  `pnpm check:versions`, verify, then tag `vX.Y.Z`.

## Notes

- `fixtures/de_ancient-2026-05-17/` is a legacy v1 fixture, skipped by validator.
- `fixtures/v3-mid/` is the v3 golden fixture (de_anubis, 21 rounds, research profile).
- Do not commit `.omc/`, `.DS_Store`, `__pycache__/`, or `node_modules/`.

## 依赖安装与更新

依赖维护遵循 [`docs/dependency-maintenance.md`](docs/dependency-maintenance.md)。使用 manifest 指定的 pnpm，日常冻结安装，版本更新时自动生成锁文件；不要手改锁文件或恢复旧文件掩盖依赖变化。

云环境中如存在 `/workspace/.onboarding/activate.sh`，每个 shell 执行仓库命令前先 source 该文件，以选择可写缓存和 manifest 对应的包管理器。
