# cs2-demo-format

**语言：** [English](./README.md) | 简体中文

`cs2-demo-format` 是一个严格、实现中立的 Counter-Strike 2 demo 解析结果 ZIP
数据合同。

它不是 demo parser、评分模型、回放播放器或 Web 应用。它是这些工具可以共享的
数据层：一个 producer 导出合法 ZIP，多个 consumer 就能校验、查看、回放、评分或
研究同一场比赛，而不用互相复制私有数据结构。

## 为什么需要这个格式

原始 `.dem` 文件很难直接消费。demo parser 能暴露有用数据，但下游项目通常都会
重复定义自己的 rounds、players、kills、damages、economy、replay 和派生统计结构。

本仓库定义这层共享结构：

- **ZIP 包结构**：解析后的 CS2 demo 数据如何组织。
- **Zod schema + TypeScript 类型**：供 JS/TS consumer 使用。
- **生成的 JSON Schema**：供 Python、Go、Rust 等非 TS 工具使用。
- **严格 parser 和 validator**：坏包应明确失败，而不是静默进入分析链路。
- **参考 Python exporter**：`cs2df` 可把 `.dem` 导出成 v3 ZIP。
- **真实 v3 fixture**：不导出 demo 也能直接查看格式样例。

合同本身位于 [`schemas/index.ts`](./schemas/index.ts) 和生成的
[`spec/*.schema.json`](./spec/)；任何 producer 只要输出合法包即符合规范。

## v3 ZIP 里有什么

v3 包是一个 ZIP，包含 `manifest.json` 以及 manifest 声明的 JSON 文件。

```text
match.zip
├── manifest.json
├── match.json
├── players.json
├── rounds.json
├── player-stats.json
├── player-economies.json
├── kills.json
├── damages.json
├── blinds.json
├── bombs.json
├── grenades.json
├── clutches.json
├── shots.json      可选
├── replay.json     可选
└── duels.json      可选，research profile
```

文件分为四组：

| 分组 | 文件 | 作用 |
|---|---|---|
| 比赛身份 | `manifest`, `match`, `players`, `rounds` | 稳定的比赛、玩家、队伍、阵营和回合时间线事实。 |
| 事件 | `kills`, `damages`, `blinds`, `bombs`, `grenades`, `clutches` | 正式回合事件行，使用 `playerIndex` 引用玩家。 |
| 聚合 | `player-stats`, `player-economies` | 每名玩家整场统计和逐回合经济快照。 |
| 流数据 | `shots`, `replay`, `duels` | 开枪事件、8 Hz 回放、满 tick 交火窗口的列式整数流。 |

每个字段和计算规则见 [`docs/field-contract.md`](./docs/field-contract.md)。

## v3.0.0 重点

- **`playerIndex` 是标准玩家引用。** `steamId64` 只出现在 `players.json`，
  其他文件引用玩家行号。
- **队伍和阵营通过推导得到。** 事件行不再重复 `teamKey` 或 `side`，consumer 从
  `players` + `rounds` 推导。
- **v3.0.3 保留 post-round 尾段。** 非最终回合的事件/replay 窗口延伸到下一回合
  开始前；`rounds.endTick` 仍表示胜负判定 tick。
- **`replay.json` 是统一状态流。** 旧 `positions-1s.json` 被移除；replay 包含
  位置、视角、血量、护甲、金钱、装备价值、当前武器、区域名、闪光和 flags。
- **列式流只存整数。** 位置、角度、金钱和装备价值均为紧凑整数数组；高频流在适合
  的地方使用差分编码。
- **`duels.json` 支持 research 导出。** 它保存 kill/damage 附近的满 tick 交火窗口，
  可用于反应时间和 duel 分析。
- **本仓库包含参考 exporter。** `cs2df` 支持导出、校验、批量处理 demo，并写出
  每场 demo 的性能报告。

## 快速开始：导出 demo

参考 exporter 在 [`python/`](./python/) 中，底层使用
[`demoparser2`](https://github.com/LaihoE/demoparser)。

```bash
cd python
uv sync

# 标准导出：必需文件 + shots.json + replay.json
uv run cs2df export match.dem

# Research 导出：额外包含满 tick duels.json
uv run cs2df export match.dem --research

# 校验结果
uv run cs2df validate match.zip --strict
```

批量导出一个 demo 目录：

```bash
uv run cs2df export-batch ./demos --workers 8 --descriptive
```

`export-batch` 会为每个 `.dem` 写出一个 ZIP，并额外生成 `report.json`，包含每场
demo 的耗时、输出大小、压缩级别、吞吐和阶段 timings。默认 ZIP 压缩级别是 `3`，
这是速度和体积之间的实测折中；如果更在意体积，可显式使用 `--compress-level 6`
或 `--compress-level 9`。

## 快速开始：消费导出包

TypeScript consumer 可以使用内置严格 parser：

```ts
import { parseDemoPackage, decodeDelta } from "cs2-demo-format/parser";
import { readFileSync } from "node:fs";

const pkg = await parseDemoPackage(readFileSync("match.zip"));

console.log(pkg.manifest.schemaVersion);
console.log(pkg.files.match.mapName);
console.log(pkg.files.players[pkg.files.playerStats[0].playerIndex].name);

const firstReplayRound = pkg.files.replay?.rounds[0];
const firstPlayerTrack = firstReplayRound?.players[0];
const decodedX = firstPlayerTrack ? decodeDelta(firstPlayerTrack.x) : [];
```

非 TypeScript consumer 使用 [`spec/`](./spec/) 中的 JSON Schema；它们由同一份
canonical Zod schema 生成。

## 示例导出

[`fixtures/v3-mid/`](./fixtures/v3-mid/) 是仓库内置的 v3 research fixture：

- 地图：`de_anubis`
- 正式回合：`21`
- profile：`--research`
- 文件：所有必需文件 + `shots.json`、`replay.json`、`duels.json`
- schema version：`cs2-demo-format/3.0`

它适合用作第一份可检查样例：

```bash
pnpm validate:fixtures
cat fixtures/v3-mid/manifest.json
cat fixtures/v3-mid/match.json
```

最大的文件是列式流（`replay.json` 和 `duels.json`），能展示 v3 导出的真实结构和规模。

## 合同保证

合法 v3 包保证：

- `schemaVersion` 必须是 `"cs2-demo-format/3.0"`。
- `players.json` 行序是规范；行号即 `playerIndex`。
- `roundNumber` 从 `1` 开始，只覆盖正式回合。
- 事件里的 `roundNumber` 必须存在于 `rounds.json`。
- 必需文件和必需字段必须存在。
- JSON 不包含裸 `NaN`、`Infinity` 或 `-Infinity`。
- 同一 columnar track 内数组长度一致。
- delta 数组用前缀和解码。
- package-level QA 会检查常见跨文件不一致。

伤害/ADR、经济分类、KAST、残局、replay 和 duel-window 语义以
[`docs/field-contract.md`](./docs/field-contract.md) 为准。

## 仓库结构

| 路径 | 作用 |
|---|---|
| [`schemas/index.ts`](./schemas/index.ts) | canonical Zod schema、TypeScript 类型和解码 helper。 |
| [`spec/`](./spec/) | 供非 TypeScript consumer 使用的生成 JSON Schema。 |
| [`parser/index.ts`](./parser/index.ts) | 参考 TypeScript ZIP parser 和 schema validator。 |
| [`python/`](./python/) | 参考 Python exporter / validator CLI（`cs2df`）。 |
| [`tools/validate.py`](./tools/validate.py) | `cs2df validate` 的薄包装。 |
| [`docs/field-contract.md`](./docs/field-contract.md) | 按文件说明字段语义和计算规则。 |
| [`fixtures/`](./fixtures/) | v3 golden fixture 和历史 legacy fixture。 |
| [`CHANGELOG.md`](./CHANGELOG.md) | 发布历史。 |

schema 变更后运行：

```bash
pnpm check:versions
pnpm gen:schema
pnpm typecheck
pnpm validate:fixtures
```

## 已知实现

- **参考 producer：** [`python/cs2df`](./python/)
- **Producer / toolkit：** [`cs2-demo-analysis-kit`](https://github.com/Starfie1d1272/cs2-demo-analysis-kit)
- **Consumer：** [`RivalHub`](https://github.com/Starfie1d1272/RivalHub)
- **原始事件提取出处：**
  [`DrEAmSs59/CS2-insight-agent`](https://github.com/DrEAmSs59/CS2-insight-agent)，
  经原作者授权移植。

## 版本规则

本包遵循 [Semantic Versioning](https://semver.org/)。

- **Major：** 必需文件、必需字段、字段删除或语义变化。
- **Minor：** 新增可选文件、可选字段或生成 schema。
- **Patch：** 不改变 ZIP 合同的文档、校验工具或 exporter 修复。

发布 tag 使用 `vX.Y.Z` 格式。
打 tag 前运行 `pnpm check:versions`，确保 `package.json`、
`python/pyproject.toml` 和 `cs2df.__version__` 一致。

## License

MIT
