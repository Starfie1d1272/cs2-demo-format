# cs2-demo-format

**语言：** [English](./README.md) | 简体中文

`cs2-demo-format` 是一个实现中立的 Counter-Strike 2 demo 解析结果数据合同。
它定义严格的 ZIP 包结构、机器可读 schema 和导出质量校验规则，让导出方、导入方和分析工具可以围绕同一份数据格式协作，而不是绑定到某一个应用。

这个格式的目标不是只服务一个 importer/exporter 组合。一个 parser 可以导出它，一个 Web 应用可以导入它，一个 rating engine 可以基于它评分，一个独立分析工具也可以校验和查询它。

当前已知实现：

- 导出方：[`DrEAmSs59/CS2-insight-agent`](https://github.com/DrEAmSs59/CS2-insight-agent)
- 导入方：[`Starfie1d1272/RivalHub`](https://github.com/Starfie1d1272/RivalHub)

## 这个包包含什么

| 路径 | 作用 |
|---|---|
| [`schemas/index.ts`](./schemas/index.ts) | canonical Zod schema 和 TypeScript 类型。这里是机器合同的单一真源。 |
| [`spec/*.schema.json`](./spec/) | 从 Zod 生成的 JSON Schema，供 Python、Go、Rust 等非 TypeScript 工具使用。 |
| [`parser/index.ts`](./parser/index.ts) | 参考 ZIP parser，读取导出包并按 schema 严格校验。 |
| [`tools/validate.py`](./tools/validate.py) | Python ZIP 校验器，可在 Node.js 之外检查导出质量。 |
| [`docs/field-contract.md`](./docs/field-contract.md) | 按文件解释每个字段的含义、计算方式、范围和 nullable 规则。 |
| [`fixtures/`](./fixtures/) | 真实样本区域。当前仓库内样本是 legacy v1，严格 v2 fixture 校验会跳过它。 |

`schemas/index.ts` 是权威来源。任何 schema 变更之后，都需要重新生成 `spec/` 并提交生成出的 JSON Schema。

## ZIP 导出结构

v2 导出包是一个 ZIP 文件，包含 `manifest.json` 以及 `manifest.files` 中声明的文件。
所有必需文件都必须存在。可选文件可以不出现在 `manifest.files` 中。

| 文件 | 是否必需 | 形态 | 作用 |
|---|---:|---|---|
| `manifest.json` | 是 | object | 包级元数据、schema 版本、源 demo 身份和文件索引。 |
| `match.json` | 是 | object | 比赛摘要：地图、tickrate、队伍槽位、比分、时长和来源。 |
| `players.json` | 是 | array | 玩家身份和稳定的 `teamKey` 归属。 |
| `rounds.json` | 是 | array | 正式回合时间线、阵营、比分状态、经济、胜方和原因。 |
| `player-stats.json` | 是 | array | 每名玩家整图聚合统计，只基于正式回合。 |
| `player-economies.json` | 是 | array | 每名玩家每回合的金钱、花费、装备价值、库存和购买类型。 |
| `kills.json` | 是 | array | 击杀事件：参与者、阵营、武器、位置、trade 标记和 duel 上下文。 |
| `damages.json` | 是 | array | 伤害事件：原始生命值伤害、有效生命值伤害、护甲伤害、受害者血量和位置。 |
| `blinds.json` | 是 | array | 闪光致盲事件：投掷者、受害者、助攻关联、持续时间和位置。 |
| `bombs.json` | 是 | array | 下包、拆包、爆炸和掉包事件。 |
| `grenades.json` | 是 | array | 投掷物生命周期事件和位置。 |
| `clutches.json` | 是 | array | 派生残局情境，关联到回合和玩家。 |
| `shots.json` | 否 | array | 逐枪事件。该文件可能很大，因此可选。 |
| `positions-1s.json` | 否 | array | 每秒玩家状态快照。该文件可能很大，因此可选。 |

## v2 严格合同

v2 有意设计为严格格式。导出包应该在写出时就是合法数据，consumer 不应该负责修复坏数据。

- `manifest.schemaVersion` 必须是 `"cs2-demo-format/2.0"`。
- `roundNumber` 只表示正式回合，从 `1` 开始，每回合加 `1`，必须连续。
- warmup 行和 round `0` 行不得出现在事件文件或聚合文件中。
- tick 字段必须是正整数。未知 tick 是导出失败，不应写成 `0` 或 `null`。
- JSON 中不得出现裸 `NaN`、`Infinity` 或 `-Infinity`。
- `teamKey` 是内部稳定槽位，只能是 `"teamA"` 或 `"teamB"`。真实队伍名在 `match.teamA.name` 和 `match.teamB.name` 中；demo 不提供时可以是 `null`。
- 正式回合中的 `side` 只能是 `"t"` 或 `"ct"`。`"unknown"` 不是合法 v2 数据。
- 必需文件和必需字段之所以必需，是因为它们不受 demo 是否提供影响。除非字段合同明确允许 `null`，缺失值都代表导出方失败。
- `null` 只用于 demo 可能真的不提供的值，例如队伍展示名或源文件 hash。它不是 parser 错误的兜底值。

## 伤害与 ADR 口径

CS2 parser 可能暴露超过受害者剩余 HP 的原始伤害。rating 系统和平台式 ADR 通常需要按剩余 HP 封顶的有效伤害。

v2 同时保存两个值：

```text
damages.healthDamageRaw = parser 原始未封顶生命值伤害
damages.healthDamage    = min(healthDamageRaw, victimHealthBefore)
```

聚合统计使用有效伤害：

```text
playerStats.damageHealth = sum(damages.healthDamage for valid enemy damage)
playerStats.adr          = playerStats.damageHealth / playerStats.rounds
playerStats.utilityAdr   = playerStats.utilityDamage / playerStats.rounds
```

self damage、team damage、world damage、bomb damage、KAST、首杀、助攻、残局和投掷物伤害的完整纳入/排除规则见
[`docs/field-contract.md`](./docs/field-contract.md)。

## 导出方要求

导出方应该把这个包当作数据合同，而不是宽松的事件日志。

一个 v2 导出方至少必须：

- 写出 ZIP 结构中列出的所有必需文件；
- 写出 `schemaVersion: "cs2-demo-format/2.0"`；
- 写入前过滤 warmup 和非正式回合行；
- 生成从 `1` 开始且连续的正式回合；
- 保证每个事件的 `roundNumber` 都存在于 `rounds.json`；
- 保证每个 SteamID、team key 和 side 都可以与 `players.json` 以及回合阵营映射对齐；
- 为每名玩家的每个正式回合写出一行 `player-economies.json`；
- 保证 `playerStats.rounds` 等于正式回合数；
- 按字段合同从基础事件文件计算 `adr`、`kast`、`utilityAdr`、首杀次数、多杀次数和残局次数；
- 遇到未知阵营、0 tick、缺失必需主体或非有限数字时让导出失败，而不是写出坏数据。

round swing、自定义 rating 权重等需要产品解释的派生指标，不应该进入基础格式层。
基础格式应保留足够原始 ingredients，让不同分析工具可以按自己的口径计算。

## Consumer 使用

TypeScript consumer 可以直接使用 schema 和参考 parser：

```ts
import { SCHEMAS_BY_KEY, type PlayerStatsRow } from "cs2-demo-format";
import { parseDemoPackage } from "cs2-demo-format/parser";
import { readFileSync } from "node:fs";

const parsed = await parseDemoPackage(readFileSync("match-export.zip"));
const stats: PlayerStatsRow[] = parsed.files.playerStats;

console.log(parsed.manifest.mapName, stats.length);
```

非 TypeScript consumer 应使用 [`spec/`](./spec/) 中生成的 JSON Schema。

## 校验

运行仓库校验：

```bash
pnpm typecheck
pnpm gen:schema
pnpm validate:fixtures
```

校验单个 ZIP 导出包：

```bash
python3 tools/validate.py match-export.zip
```

如果本机 pnpm 策略阻止依赖 build scripts，先批准一次需要的 build：

```bash
pnpm approve-builds
```

校验器不只检查 JSON Schema，也会做包级 QA，包括回合连续性、事件回合缺失、经济覆盖、聚合统计对齐、异常 tick、无法解析的 SteamID、无法对齐的 team/side，以及 damage/ADR 一致性。

## 版本规则

这个包遵循 Semantic Versioning。

- Major：新增必需文件、必需字段变化、字段删除或语义变化。
- Minor：新增可选字段、新增可选文件、schema 追加或非破坏性校验增强。
- Patch：文档修正、不改变合同的生成 schema 修正和工具修正。

发布 tag 使用 `vX.Y.Z` 格式，例如 `v2.0.0`。

## 文档索引

- 完整字段合同：[`docs/field-contract.md`](./docs/field-contract.md)
- JSON Schema 输出：[`spec/`](./spec/)
- fixture 说明：[`fixtures/README.md`](./fixtures/README.md)
- 发布历史：[`CHANGELOG.md`](./CHANGELOG.md)

## License

MIT
