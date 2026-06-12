# Changelog

All notable changes to cs2-demo-format are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) · [Semver](https://semver.org/)

## 3.0.0

### Breaking

- **`playerIndex` 替代 `steamId64`**：除 `players.json` 外，所有文件的玩家引用
  改为 `players.json` 的 0-based 数组索引。17 位 steamId64 仅出现一次。
- **事件行移除 `teamKey` / `side`**：由 `players[playerIndex].teamKey` +
  `rounds[roundNumber].teamASide/teamBSide` 推导。
- **删除 `positions-1s.json`**：合并入 `replay.json`，统一为 8 Hz 列式全状态流
  （新增 pitch / armor / money / equipValue / flash / place / flags 列）。
- **`kast_rounds` → `kastRounds`**（snake_case → camelCase）。
- **`damages.victimHealthAfter` 移除**（= before − healthDamage，纯算术冗余）。
- **`damages.victimArmorBefore` 移除**（= after + armorDamage，纯算术冗余）。
- **`bombs.siteId` 移除**（与 site 冗余）。
- **`teamEconomyType` 移除 `"conversion"`**：手枪局胜方下一轮队伍经济输出
  `"full"`，conversion 语义隐含于 roundNumber 与前轮 winnerTeamKey。
- **`shots.json` 重构**：从行式改为列式（按 round/player 分组 track，差分编码）。

### Added

- **差分编码**：replay / duels / shots 中位置、角度、经济序列全部整数差分编码，
  解码用运行前缀和。
- **纯整数流**：三个高频流零浮点。角度存 `度 × angleScale`（默认 10 = 0.1°），
  闪光存 0.1 秒单位，坐标存整数游戏单位。
- **`duels.json`**：满 tick 交火窗口流，供反应时间测定。以 kill/damage 事件为锚，
  合并 `[tick − windowBeforeMs, tick + windowAfterMs]` 重叠窗口，窗口内所有存活
  玩家以原生 tickrate 采样。可选（research profile）。
- **`replay.json` 扩展**：新增 `placeDict` + `place` 列（CS2 callout 区域名），
  `equipValue` 列（装备价值），`angleScale` 元参数。`money` 列改为真正的现金余额
  （v2 positions-1s 的 money 实际存的是 equipValue）。
- **`replayPlayerTrack.flags`** 精简为 3 位：1=alive, 2=hasBomb, 4=hasDefuseKit。
- **`decodeDelta()` 解码辅助函数**，由 `schemas/index.ts` 导出，`parser/index.ts`
  再导出。
- **参考 Python 导出 CLI** (`python/src/cs2df/`)，uv 管理。向量化
  DataFrame→numpy→delta 管线、orjson 序列化、默认 deflate level 3。命令：
  `cs2df export` / `cs2df export-batch` / `cs2df validate`。hasBomb 由炸弹事件
  状态机推导（不再逐帧解析 inventory），大解析阶段性能大幅提升。
- **批量导出性能报告**：`export-batch` 写出 `report.json`，包含每场 demo 的
  成功/失败、输出大小、压缩级别、吞吐和 parse/package/write 阶段 timings。
- **批量导出失败隔离**：单个损坏 demo 或 native parser 失败记录为失败行，不再让
  整个 batch 进程 traceback 崩溃。
- **v3 golden fixture** (`fixtures/v3-mid/`)：de_anubis, 21 回合, research profile。

### Changed

- `manifest.schemaVersion` → `"cs2-demo-format/3.0"`。
- `replay.meta` 新增 `angleScale`（必填），删除 `sampleRate`、`tickrate` 仍为必填。
- `replay.projectiles` 改为必填（v2.3 为 optional）；无数据时为空数组。
- `tools/validate.py` 重写为 `cs2df validate` 薄包装。
- `scripts/validate-fixtures.ts` 适配 v3 schema + 列式流 QA。
- 所有文档更新至 v3。

## 2.3.0

### Minor Changes

- positions-1s 与 replay 补全空间/回放地基（加性、向后兼容）：

  - **positions-1s 新增 `lastPlaceName`**（可选、可空）：CS2 自带 callout 区域名，
    供 Area 占有 / 动线分析按区域聚合，免人工多边形标定。
  - **replay 每回合新增 `projectiles`**（可选）：每颗道具的逐帧飞行弧线
    （`grenade` / `throwerSteamId64` / `startTick` / `x[]`/`y[]`/`z[]`），与选手轨迹同
    时间网格对齐，供 2D 回放渲染。仅飞行段；静态烟/火效果仍在 grenades.json。

    2.3.0 之前导出的包不带这两个字段，仍合法校验。

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
  - `full` — `equipment_value >= 4000` (AK + armor + util baseline)
  - `eco` — `money_spent < 1000` AND `equipment_value < 2000`
  - `force` — `start_money > 0` AND `money_spent / start_money > 0.75`
  - `semi` — everything else (fallback)
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
