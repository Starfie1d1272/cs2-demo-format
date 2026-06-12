# cs2-demo-format

**语言：** [English](./README.md) | 简体中文

`cs2-demo-format` 是实现中立的 Counter-Strike 2 demo 解析结果数据合同。它定义
严格的 ZIP 包结构、机器可读 schema 和导出质量校验规则，让导出方、导入方和分析
工具可以围绕同一份数据格式协作，而不是绑定到某一个应用。

这个格式的目标不是只服务一个 importer/exporter 组合——一个 parser 可以导出它，
一个 Web 应用可以导入它，一个 rating engine 可以基于它评分，一个独立分析工具也
可以校验和查询它。合同本身在 `schemas/index.ts` + `spec/*.schema.json`，任何
producer 只要输出合法包即符合规范。

当前已知实现：

- **导出方**：[`cs2df`](./python/)（本仓库内置参考 CLI）、
  [`cs2-demo-analysis-kit`](https://github.com/Starfie1d1272/cs2-demo-analysis-kit)
- **导入方**：[`RivalHub`](https://github.com/Starfie1d1272/RivalHub)
- 原始出处：事件提取逻辑源自
  [`DrEAmSs59/CS2-insight-agent`](https://github.com/DrEAmSs59/CS2-insight-agent)，
  经原作者授权移植。

## 本仓库包含什么

| 路径 | 作用 |
|---|---|
| [`schemas/index.ts`](./schemas/index.ts) | canonical Zod schema + TypeScript 类型。单一真源。 |
| [`spec/*.schema.json`](./spec/) | 从 Zod 生成的 JSON Schema，供 Python/Go/Rust 等非 TS 工具使用。 |
| [`parser/index.ts`](./parser/index.ts) | 参考 TypeScript ZIP parser，按 schema 严格校验。 |
| [`python/`](./python/) | 参考 Python 导出 CLI（`cs2df export` / `cs2df validate`）。 |
| [`tools/validate.py`](./tools/validate.py) | 轻量 Python 校验包装（→ `cs2df validate`）。 |
| [`docs/field-contract.md`](./docs/field-contract.md) | 按文件解释每个字段语义、计算规则和 v2→v3 迁移指南。 |
| [`fixtures/`](./fixtures/) | Golden fixture（`fixtures/v3-mid/`——de_anubis，21 回合，research profile）。 |

`schemas/index.ts` 是权威来源。schema 变更后需运行 `pnpm gen:schema` 并提交更新
的 `spec/` 文件。

## ZIP 包结构 (v3)

v3 导出包是一个 ZIP 文件，包含 `manifest.json` 及其声明的数据文件。

| 文件 | 必需 | 形态 | 作用 |
|---|---:|---|---|
| `manifest.json` | 是 | object | 包元数据、schema 版本（`"cs2-demo-format/3.0"`）、demo 身份、文件索引。 |
| `match.json` | 是 | object | 比赛摘要：地图、tickrate、队伍槽位、比分、时长。 |
| `players.json` | 是 | array | 玩家身份 + `teamKey`。**行序即规范**——行号即全包通用的 `playerIndex`。 |
| `rounds.json` | 是 | array | 正式回合时间线、阵营、比分状态、队伍经济（多数投票）、胜方、结束原因。 |
| `player-stats.json` | 是 | array | 每名玩家聚合统计（击杀、ADR、KAST、多杀、残局、闪光等）。 |
| `player-economies.json` | 是 | array | 每玩家每回合经济快照（金钱、花费、装备、购买类型）。行数 = rounds × players。 |
| `kills.json` | 是 | array | 击杀事件：参与者用 `playerIndex`、武器、位置、trade/flash/smoke 标记。 |
| `damages.json` | 是 | array | 伤害事件：原始+封顶有效生命值伤害、护甲伤害、命中部位、位置。 |
| `blinds.json` | 是 | array | 闪光致盲事件：投掷者、被闪者、时长、flashId 关联。 |
| `bombs.json` | 是 | array | 炸弹生命周期：`plant_begin`、`planted`、`defuse_begin`、`defused`、`exploded`、`dropped`、`picked_up`。 |
| `grenades.json` | 是 | array | 投掷物投出/生效事件：投掷者、位置、时序、destroy tick。 |
| `clutches.json` | 是 | array | 派生 1vN 残局：残局者、对手数、won/survived/killCount。 |
| `shots.json` | 否 | columnar | 开枪流，按 (round, playerIndex) 分组 track，差分编码。 |
| `replay.json` | 否 | columnar | 统一 8 Hz 玩家状态流——位置、视角、血量、护甲、金钱、装备、武器、callout 区域名、闪光、flags。合并原 `positions-1s.json`。 |
| `duels.json` | 否 | columnar | 满 tick 交火窗口流，供反应时间分析。可选，`--research` 开关。 |

## v3 核心变更

- **`playerIndex` 替代 `steamId64`**：SteamID64 字符串仅在 `players.json` 出现一次。
- **事件行移除 `teamKey` / `side`**——由 `players[playerIndex].teamKey` +
  `rounds[roundNumber]` 推导。
- **`positions-1s.json` 合并入 `replay.json`**——统一 8 Hz 列式流，保留原
  positions-1s 全部字段（pitch、armor、money、equipValue、flash、place、flags）。
- **差分编码**：所有列式流中位置/角度/经济序列使用整数差分编码，解码用运行前缀和
  （`decodeDelta()` 辅助函数已导出）。
- **纯整数列**：`replay.json`、`duels.json`、`shots.json` 零浮点。角度存
  `度 × angleScale`（默认 0.1°），闪光存 0.1 秒单位，坐标存整数游戏单位。
- **字段清理**：`kast_rounds` → `kastRounds`；移除冗余字段
  `damages.victimHealthAfter` / `victimArmorBefore`；移除 `bombs.siteId`。
- **`duels.json`**（research profile）——以 kill/damage 为锚的满 tick 交火窗口，
  供反应时间测定。
- **`shots.json`** 从行式重构为列式 track。
- 完整 v2→v3 迁移表见 [`docs/field-contract.md`](./docs/field-contract.md)。

## 基础规则（跨版本适用）

以下规则不随版本变化，是格式的基线合同：

- `roundNumber` 从 `1` 开始，连续递增。warmup / round 0 数据不得出现在事件或聚合文件中。
- tick 字段必须为正整数。tick 缺失是导出方错误，不可写 `0` 或 `null`。
- JSON 中不得出现裸 `NaN`、`Infinity` 或 `-Infinity`。
- `null` 仅用于 demo 确实可能不提供的值（如队伍展示名、demo hash），不可作为 parser 错误的兜底值。
- 必需文件和必需字段之所以必需，是因为它们不受 demo 是否提供影响。缺失值代表导出方失败。

## 伤害与 ADR 口径

CS2 parser 可能暴露超过受害者剩余 HP 的原始伤害。rating 系统和平台式 ADR 使用
按剩余 HP 封顶的有效伤害：

```text
damages.healthDamage    = min(healthDamageRaw, victimHealthBefore)
playerStats.damageHealth = Σ damages.healthDamage  （仅 anti-enemy）
playerStats.adr          = damageHealth / rounds
```

投掷物伤害（HE、火）使用相同封顶口径。

## 快速开始

### 导出 demo（Python）

```bash
cd python && uv sync                     # 一次性初始化
uv run cs2df export match.dem            # → match.zip（标准 8 Hz replay）
uv run cs2df export match.dem --research # + duels.json（满 tick 交火窗口）
```

### 消费导出包（TypeScript）

```ts
import { type PlayerStatsRow } from "cs2-demo-format";
import { parseDemoPackage, decodeDelta } from "cs2-demo-format/parser";
import { readFileSync } from "node:fs";

const pkg = await parseDemoPackage(readFileSync("match.zip"));
console.log(pkg.manifest.mapName, pkg.files.playerStats.length);
```

非 TypeScript consumer 使用 [`spec/`](./spec/) 中的 JSON Schema。

## 校验

```bash
# 仓库级校验
pnpm typecheck
pnpm gen:schema
pnpm validate:fixtures

# 校验单个导出 ZIP
uv run cs2df validate export.zip
python3 tools/validate.py export.zip   # 薄包装
```

## 导出方要求

符合规范的 v3 导出方必须：

- 写出所有必需文件及 `schemaVersion: "cs2-demo-format/3.0"`。
- 以稳定顺序写出 `players.json`（建议：teamKey 再 steamId64），全包以此顺序为 `playerIndex`。
- 写入前过滤 warmup 和非正式回合。
- 生成从 `1` 开始连续的 `roundNumber`。
- 确保每个事件的 `roundNumber` 存在于 `rounds.json`。
- 为每名玩家的每个正式回合写出一行 `player-economies.json`。
- 确保 `playerStats.rounds == rounds.length`。
- 按 [`docs/field-contract.md`](./docs/field-contract.md) 中的规则，从事件文件计算 ADR、KAST、首杀/多杀/残局次数。
- 列式流中位置/角度/经济序列使用差分编码。
- 遇到未知阵营、零 tick、缺失必需主体或非有限数字时让导出失败，而不是写出坏数据。

## 版本规则

本包遵循 [Semantic Versioning](https://semver.org/)。

- **Major** — 新增必需文件、必需字段变化、字段删除或语义变化。
- **Minor** — 新增可选字段/文件、schema 追加。
- **Patch** — 文档修正、工具修正。

发布 tag 使用 `vX.Y.Z` 格式（如 `v3.0.0`）。

## 文档索引

- 完整字段合同：[`docs/field-contract.md`](./docs/field-contract.md)
- JSON Schema：[`spec/`](./spec/)
- 参考导出器：[`python/`](./python/)
- Fixtures：[`fixtures/`](./fixtures/)
- 发布历史：[`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT
