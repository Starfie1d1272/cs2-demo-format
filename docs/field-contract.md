# CS2 Demo Export ZIP — v3 Field Contract

本文档描述 v3 严格导出 ZIP 包中每个文件、字段、语义和计算规则。
Machine validation 由 `spec/*.schema.json` (生成自 `schemas/index.ts`) 承担。
README 只作为上手入口；字段级语义、派生指标和 producer 要求以本文档为准。

Schema version: `cs2-demo-format/3.0`

---

## v3 Core Conventions

### 玩家引用：`playerIndex`

除 `players.json` 外，所有文件通过 `playerIndex`（`players.json` 数组的 0-based
行号）引用玩家。17 位 `steamId64` 字符串仅在 `players.json` 中出现一次。

### teamKey / side 推导（v2 字段在 v3 中已移除）

```
teamKey  = players[playerIndex].teamKey
side     = rounds[roundNumber].teamASide  当 teamKey == "teamA"
           rounds[roundNumber].teamBSide  当 teamKey == "teamB"
```

### 整数流

`replay.json`、`duels.json`、`shots.json` 不含任何浮点数。所有值均为整数：
- 坐标：游戏单位 × `coordScale`（默认 1）
- 角度：度数 × `angleScale`（默认 10 → 0.1° 精度）
- 闪光时长：0.1 秒为单位（0–60）
- 金钱：整数

事件文件（`damages.json`、`blinds.json` 等）中 duration/ratio 字段仍可使用 float。

### 差分编码 (delta)

标记为 "delta" 的数组首元素为绝对值，后续元素为与前帧之差：
```
encoded[0] = value[0]
encoded[i] = value[i] − value[i−1]   (i > 0)
```
用运行前缀和解码。非 delta 数组存每帧原始值。

### NaN / Infinity

禁止在任何 JSON 值中出现。必须转为 `null` 或合法 sentinel。

---

## manifest.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `schemaVersion` | `"cs2-demo-format/3.0"` | |
| `exporter.name` / `exporter.version` | string | 导出工具标识 |
| `parser.name` / `parser.version` | string | 底层 demo parser |
| `demo.hash` | string(64 hex) 或 null | 源 .dem 的 SHA-256 |
| `demo.sourceFileName` | string 或 null | 源 .dem 文件名 |
| `mapName` | string | 地图名 |
| `tickrate` | integer ≥ 1 | |
| `exportedAt` | ISO-8601 string (UTC) | |
| `files` | object | key→filename 映射 |

Required keys: `match`, `players`, `rounds`, `playerStats`, `playerEconomies`,
`kills`, `damages`, `blinds`, `bombs`, `grenades`, `clutches`.
Optional: `shots`, `replay`, `duels`。

---

## match.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `mapName` | string | 同 manifest |
| `tickrate` | integer ≥ 1 | |
| `durationSeconds` | number > 0 | 由 header playback_time 或 last endTick/tickrate 得出 |
| `serverName` | string 或 null | |
| `source` | `"demo"` | |
| `teamA` / `teamB` | TeamSummary | `{ teamKey, name: string\|null, score }`，score 须等于 winnerTeamKey==该队 的回合数 |

---

## players.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `steamId64` | string (17 位) | |
| `name` | string | |
| `teamKey` | `"teamA"` 或 `"teamB"` | |

**行序是规范性的**——行号即全包通用的 `playerIndex`。建议按 teamKey 再 steamId64 排序以保证确定性。

---

## rounds.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | 从 1 开始连续 |
| `startTick` | integer ≥ 1 | `round_start`，本回合冻结期开始 |
| `freezeEndTick` | integer ≥ 1 | `round_freeze_end`，冻结期结束（active play 开始） |
| `endTick` | integer ≥ 1 | `round_end`，本回合胜负判定 tick |
| `teamASide` / `teamBSide` | `"t"` 或 `"ct"` | 必须互异 |
| `teamAScoreBefore` / `teamBScoreBefore` | integer ≥ 0 | 进入本回合前比分 |
| `teamAEconomy` / `teamBEconomy` | economy type | 队伍经济分类（5 名队员多数投票） |
| `winnerTeamKey` | `"teamA"` 或 `"teamB"` | |
| `winnerSide` | `"t"` 或 `"ct"` | 必须匹配 winnerTeamKey 的阵营 |
| `endReason` | end reason | |

Tick order: `startTick < freezeEndTick ≤ endTick`.

### 经济分类算法

**个人经济** (`pistol` / `eco` / `semi` / `force` / `full`)，优先级从高到低：

1. `pistol` — `roundNumber` 为 1 或 13（MR12 手枪局；加时不适用）
2. `full` — `equipmentValue >= 4000`
3. `eco` — `moneySpent < 1000` 且 `equipmentValue < 1000`
4. `force` — `startMoney > 0` 且 `moneySpent / startMoney >= 0.80`
5. `semi` — 其余情况

**队伍经济**：5 名队员个人类型多数投票，平局按 `pistol < eco < semi < force < full` 取高。

**手枪局转换轮次** (R2 / R14 中赢下前一个手枪局的队伍)：队伍经济固定为 `full`。
设计意图：手枪局胜方经济上相对于输方已是"长枪局"级别优势；输方通常 eco/force。
"conversion" 的语义隐含在 roundNumber 和前轮 winnerTeamKey 中，不单独输出枚举值。

**End reason**: `t_win` / `ct_win` / `target_bombed` / `bomb_defused` / `time_ran_out`.

---

## player-stats.json

按玩家聚合的整场统计。所有计数字段为非负整数。

| 字段 | 说明 |
|---|---|
| `playerIndex` | players.json 行号 |
| `rounds` | 必须 = rounds.length |
| `kills` / `deaths` / `assists` | 有效击杀/死亡/助攻 |
| `damageHealth` | 封顶有效生命值伤害合计（仅 anti-enemy） |
| `damageArmor` | 护甲伤害合计（仅 anti-enemy） |
| `adr` | damageHealth / rounds |
| `utilityDamage` | HE/火 造成的有效生命值伤害 |
| `averageUtilityDamagePerRound` | utilityDamage / rounds |
| `headshotCount` | 爆头击杀 |
| `firstKillCount` / `firstDeathCount` | 首杀/首死回合数 |
| `tradeKillCount` / `tradeDeathCount` | trade kill/death 数 (6s 窗口) |
| `kast` | kastRounds / rounds × 100 (%) |
| `kastRounds` | 有 kill、assist、survive 或 trade 的回合数 |
| `oneKillCount`…`fiveKillCount` | exactly N kills 的回合数 |
| `vsOneCount`…`vsFiveCount` | 1vN 残局次数 |
| `vsOneWonCount`…`vsFiveLostCount` | 残局胜负 |
| `bombPlantCount` / `bombDefuseCount` | 下包/拆包次数 |
| `wallbangKillCount` / `noScopeKillCount` / `collateralKillCount` | |
| `flashAssistCount` | 闪光助攻 |
| `enemyFlashDurationSeconds` | 闪到敌人的总秒数 |
| `teamFlashDurationSeconds` | 闪到队友的总秒数 |
| `combatDeathCount` | 战斗死亡（有 killer） |
| `bombDeathCount` | C4 爆炸死亡（无 killer） |

---

## player-economies.json

每行 = 一个玩家在一个回合的经济快照。应有 `rounds.length × players.length` 行。

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | |
| `playerIndex` | integer ≥ 0 | |
| `startMoney` | integer ≥ 0 | 回合开始金钱 |
| `moneySpent` | integer ≥ 0 | 本回合花费 |
| `equipmentValue` | integer ≥ 0 | 装备价值 |
| `type` | economy type | 个人经济分类（算法见 rounds.json 节） |
| `hasArmor` / `hasHelmet` / `hasDefuseKit` | boolean | |
| `primaryWeapon` / `secondaryWeapon` | string 或 null | 武器 display name |
| `grenadeCount` | integer ≥ 0 | 持有道具数量 |
| `grenades` | grenade type array（可缺省） | 回合 freeze time 的持有道具类型；旧 v3 导出可缺省 |

---

## kills.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | |
| `tick` | integer ≥ 1 | 在本回合事件窗口内：非最终回合为 `[freezeEndTick, nextRound.startTick)`，最终回合回退为 `[freezeEndTick, endTick]` |
| `killerIndex` | integer 或 null | null = world/bomb kill |
| `victimIndex` | integer ≥ 0 | |
| `assisterIndex` | integer 或 null | |
| `flashAssisterIndex` | integer 或 null | 仅在 flashAssist=true 时非 null |
| `weapon` | string | 击杀武器 |
| `killerActiveWeapon` / `victimActiveWeapon` | string 或 null | 击杀瞬间手持武器 |
| `headshot` / `flashAssist` / `tradeKill` / `tradeDeath` | boolean | |
| `throughSmoke` / `noScope` | boolean | |
| `penetratedObjects` | integer ≥ 0 | |
| `killerPosition` | vec3 或 null | 整数坐标 |
| `victimPosition` | vec3 | 整数坐标 |

---

## damages.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | |
| `tick` | integer ≥ 1 | |
| `attackerIndex` | integer 或 null | null = world damage |
| `victimIndex` | integer ≥ 0 | |
| `weapon` | string | |
| `hitgroup` | hitgroup enum | |
| `healthDamage` | integer ≥ 0 | `min(healthDamageRaw, victimHealthBefore)` |
| `healthDamageRaw` | integer ≥ 0 | parser 原始伤害 |
| `armorDamage` | integer ≥ 0 | |
| `victimHealthBefore` | integer 0–100 | |
| `victimArmorAfter` | integer 0–100 | |
| `attackerPosition` | vec3 或 null | 整数坐标 |
| `victimPosition` | vec3 | 整数坐标 |

v3 移除: `victimHealthAfter` (= before − healthDamage)、`victimArmorBefore` (= after + armorDamage)，纯算术冗余。

---

## blinds.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | |
| `tick` | integer ≥ 1 | |
| `flashId` | string 或 null | 造成此盲的 flashbang 的 grenadeId |
| `flasherIndex` | integer ≥ 0 | 投掷闪光者 |
| `flashedIndex` | integer ≥ 0 | 被闪者 |
| `durationSeconds` | float 0–6 | 致盲时长（秒） |

---

## bombs.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | |
| `tick` | integer ≥ 1 | |
| `type` | bomb event type | |
| `site` | `"a"` / `"b"` 或 null | 由 actor 的 last_place_name 推出 |
| `actorIndex` | integer 或 null | 执行者；非玩家事件为 null |
| `position` | vec3 | 整数坐标 |

Bomb types: `plant_begin` / `planted` / `defuse_begin` / `defused` / `exploded` / `dropped` / `picked_up`.

v3 移除: `siteId`（per-map 实体索引，与 site 冗余）。

---

## grenades.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | |
| `grenadeId` | string 或 null | entity-id + throwTick 复合 |
| `throwTick` | integer ≥ 1 | 投出 tick |
| `effectTick` | integer ≥ 1 | 生效/引爆 tick |
| `destroyTick` | integer ≥ 1 或 null | 效果结束 tick（烟/火） |
| `grenade` | grenade type | |
| `throwerIndex` | integer ≥ 0 | |
| `throwPosition` | vec3 | 投掷者位置（整数坐标） |
| `effectPosition` | vec3 | 生效位置（整数坐标） |

---

## clutches.json

| 字段 | 类型 | 说明 |
|---|---|---|
| `roundNumber` | integer ≥ 1 | |
| `tick` | integer ≥ 1 | 首次检测到 1vN 的 tick |
| `clutcherIndex` | integer ≥ 0 | |
| `opponentCount` | integer 1–5 | |
| `won` / `survived` | boolean | |
| `killCount` | integer 0–5 | 从该时刻起 clutcher 的击杀数 |

---

## shots.json（列式开枪流）

可选。按 (round, player) 分组的列式 track。全部整数；`tick`/`x`/`y`/`z`/`yaw`/`pitch` 为 delta。

| 字段 | 说明 |
|---|---|
| `meta.coordScale` / `meta.angleScale` | |
| `weaponDict` | 武器名字典；-1 = 无 |
| `tracks[].roundNumber` / `tracks[].playerIndex` | |
| `tracks[].tick` | delta；每次开枪的绝对 tick |
| `tracks[].weapon` | weaponDict 索引 |
| `tracks[].x` / `y` / `z` | delta；开枪者位置 |
| `tracks[].vx` / `vy` / `vz` | 速度（非 delta，帧间无相关性） |
| `tracks[].yaw` / `pitch` | delta；视角角度 × angleScale |

同一 track 内所有数组等长。

---

## replay.json（统一列式玩家状态流）

包的主位置/状态流，默认 8 Hz（每 tickrate/8 tick 一帧）。每回合每玩家一条 track，
`frameCount` 个平行数组；帧 i 的 tick = `startTick + i × tickStep`。
Replay track 从本回合 `freezeEndTick` 开始；非最终回合延伸到下一回合
`startTick` 之前的最后一个采样 tick，用于保留回合结算、缴枪、捡枪等 post-round
尾帧。`rounds.json.endTick` 表示胜负判定 tick，不是非最终回合事件窗口的结束点。

除 `grenades` 外全部整数；`x`/`y`/`z`/`yaw`/`pitch`/`money`/`equipValue` 为 delta；
`hp`/`armor`/`flash`/`flags`/`weapon`/`place`/`grenades` 存每帧原始值。

| Meta 字段 | 说明 |
|---|---|
| `sampleRate` | 每秒帧数（如 8） |
| `tickrate` | |
| `coordScale` / `angleScale` | |

| Track 列 | 编码 | 说明 |
|---|---|---|
| `playerIndex` | scalar | |
| `x` / `y` / `z` | delta | 坐标 / coordScale |
| `yaw` / `pitch` | delta | 视角 × angleScale |
| `hp` | plain | 血量 0–100 |
| `armor` | plain | 护甲 0–100 |
| `money` | delta | 现金余额 |
| `equipValue` | delta | 装备价值 |
| `weapon` | plain | weaponDict 索引；-1 = 无 |
| `place` | plain | placeDict 索引（CS2 callout 名）；-1 = 无名区域 |
| `flash` | plain | 剩余致盲时长，0.1 秒单位（0–60） |
| `flags` | plain | 位字段：1=alive, 2=hasBomb, 4=hasDefuseKit |
| `grenades` | plain | 每帧当前持有道具类型数组；空数组 = 无或未知；旧 v3 导出可缺省 |

**weaponDict / placeDict**：全局字符串字典，存于 replay.json 顶层。

**projectiles**：每回合可选的投掷物飞行轨迹数组，同一时间网格上渲染用。`x`/`y`/`z` 为 delta。

---

## duels.json（满 tick 交火窗口流，research profile）

可选。以击杀/伤害事件为锚点的高频采样窗口，用于反应时间测定与交火分析。
采样率 = tickrate（tickStep = 1）。

窗口构建：每个 kill/damage 事件生成 `[tick − windowBeforeMs, tick + windowAfterMs]`
区间；同回合内重叠区间合并。

| Meta 字段 | 说明 |
|---|---|
| `tickrate` / `sampleRate`(=tickrate) / `coordScale` / `angleScale` | |
| `windowBeforeMs` / `windowAfterMs` | 窗口前后范围（毫秒） |

| Window 字段 | 说明 |
|---|---|
| `roundNumber` / `startTick` / `tickStep` / `frameCount` | |
| `anchors` | 触发此窗口的战斗事件数组 |
| `players` | 窗口内存活的所有玩家 track |

| Track 列 | 编码 | 说明 |
|---|---|---|
| `playerIndex` | scalar | |
| `x` / `y` / `z` | delta | 坐标 |
| `yaw` / `pitch` | delta | 视角 |
| `hp` | plain | 血量 |
| `flash` | plain | 致盲时长 (0.1s) |

| Anchor 字段 | 说明 |
|---|---|
| `kind` | `"kill"` 或 `"damage"` |
| `tick` | 事件 tick |
| `attackerIndex` / `victimIndex` | |

**反应时间分析**：结合满 tick 位置+视角 + shots.json 的精确开枪 tick，可计算：
视觉 onset（敌人进入视角锥的时刻）→ 首次开枪的延迟。
用 `flash` 列排除被闪样本。

---

## v2 → v3 迁移指南

| v2 | v3 |
|---|---|
| 事件行中 `steamId64` | `playerIndex`（0-based 索引） |
| 事件行中 `teamKey` / `side` | 由 `players[playerIndex]` + `rounds[roundNumber]` 推导 |
| `positions-1s.json` | 合并入 `replay.json`（8 Hz 全状态流） |
| `kast_rounds` | `kastRounds` |
| `damages.victimHealthAfter` | 移除 (= before − healthDamage) |
| `damages.victimArmorBefore` | 移除 (= after + armorDamage) |
| `bombs.siteId` | 移除（与 site 冗余） |
| teamEconomyType 含 `"conversion"` | 移除；conversion 轮次输出 `"full"`，语义隐含于 roundNumber |
| replay track `x`/`y`/`z` 绝对值 | 差分编码 |
| replay track `steamId64` / `teamKey` / `side` | `playerIndex` |
| 浮点坐标 | 整数游戏单位 |
| replay.meta 无 `angleScale` | 新增 `angleScale`（默认 10） |
| — | `duels.json`（research profile） |
| `shots.json` 行式 | 列式，差分编码 |
