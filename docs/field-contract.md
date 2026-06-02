# CS2 Demo Export ZIP 严格字段合同

本文档描述严格导出的 ZIP 包：里面有哪些 JSON 文件、每个文件的作用、
每个文件包含哪些字段、每个字段的含义、计算方式和范围限制。
旧导出包可以由迁移工具兼容；新的 producer 不应为了兼容坏数据放宽这些约束。

## ZIP 文件清单

| 文件 | 是否必需 | 数据形态 | 作用 |
|---|---:|---|---|
| `manifest.json` | 是 | object | 包级元数据和文件索引。 |
| `match.json` | 是 | object | 比赛级摘要，例如地图、tickrate、队伍名、最终比分。 |
| `players.json` | 是 | array | 本场比赛玩家列表和队伍归属。 |
| `rounds.json` | 是 | array | 每个正式回合的开始、结束、比分、阵营、胜负和经济摘要。 |
| `player-stats.json` | 是 | array | 每名玩家整张地图的聚合统计。 |
| `player-economies.json` | 是 | array | 每名玩家每个正式回合的经济快照。 |
| `kills.json` | 是 | array | 每次击杀事件。 |
| `damages.json` | 是 | array | 每次伤害事件。 |
| `blinds.json` | 是 | array | 每次闪光致盲事件。 |
| `bombs.json` | 是 | array | 炸弹相关事件，例如下包、拆包、爆炸。 |
| `grenades.json` | 是 | array | 投掷物投掷和生效事件。 |
| `clutches.json` | 是 | array | 残局事件，属于可复算的派生事件。 |
| `shots.json` | 否 | array | 逐枪事件，体积较大。 |
| `positions-1s.json` | 否 | array | 每秒玩家状态快照，体积很大。 |

## 通用字段约定

| 字段 / 类型 | 含义和范围 |
|---|---|
| `roundNumber` | 正式回合编号，从 1 开始连续递增。warmup / round 0 不应进入正式导出文件。 |
| `tick` | demo tick，正整数。未知 tick 代表解析失败；不要用 `0` 或 `null` 伪造未知。 |
| `steamId64` | Steam 64-bit ID，十进制字符串，必须满足 `^\d{17}$`。world、bomb、fall 等非玩家主体可为 `null`。 |
| `teamKey` | 导出包内稳定队伍标识，固定为 `teamA` 或 `teamB`。真实队伍名称在 `match.teamA.name` / `match.teamB.name`。 |
| `side` | `"t"` 或 `"ct"`。正式导出中不允许 `unknown`；出现 unknown 代表解析失败。 |
| `vec3` | CS2 世界坐标 `{ "x": number, "y": number, "z": number }`。字段允许为 null 时，表示整个坐标未知；坐标组件不应为 null。 |
| `percentage` | 百分数使用 `[0, 100]`，例如 KAST 为 `73.5`，不是 `0.735`。 |
| nullable 字段 | 只有 demo 本身可能不提供、主体可能不是玩家、或事件天然不适用时才允许 `null`。解析器本应能得到的信息不得为 `null`。 |

## manifest.json

作用：描述导出包自身，并提供逻辑文件 key 到 ZIP 内文件名的映射。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `schemaVersion` | string，非 null | 格式版本。 | producer 写入。 | 当前为 `"cs2-demo-format/2.0"`。 |
| `exporter.name` | string，非 null | 导出工具名称。 | producer 写入。 | 非空字符串。 |
| `exporter.version` | string，非 null | 导出工具版本。 | producer 写入。 | 非空字符串。 |
| `parser.name` | string，非 null | 底层 demo parser 名称。 | producer 写入。 | 非空字符串。 |
| `parser.version` | string，非 null | 底层 demo parser 版本。 | producer 写入。 | 字符串。 |
| `demo.hash` | string 或 null，非缺省 | 源 demo 文件 hash。 | producer 计算；无法计算时为 null。 | 若非 null，必须是 sha256 64 位 hex。 |
| `demo.sourceFileName` | string 或 null，非缺省 | 源 demo 文件名。 | producer 写入；不可得时为 null。 | 字符串。 |
| `mapName` | string，非 null | 地图名。 | demo metadata。 | 例如 `de_ancient`；应与 `match.mapName` 一致。 |
| `tickrate` | int，非 null | demo tickrate。 | demo metadata。 | 正整数。 |
| `exportedAt` | string，非 null | 导出时间。 | producer 写入。 | ISO 8601。 |
| `files` | object，非 null | 逻辑 key 到文件名的映射。 | producer 写入。 | 必需文件必须都声明并存在。 |

`files` 必须包含 required 文件；可选文件存在时也必须在这里声明：

```json
{
  "match": "match.json",
  "players": "players.json",
  "rounds": "rounds.json",
  "playerStats": "player-stats.json",
  "playerEconomies": "player-economies.json",
  "kills": "kills.json",
  "damages": "damages.json",
  "blinds": "blinds.json",
  "bombs": "bombs.json",
  "grenades": "grenades.json",
  "clutches": "clutches.json",
  "shots": "shots.json",
  "positions1s": "positions-1s.json"
}
```

## match.json

作用：描述比赛级摘要。该文件是单个 object，不是数组。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `mapName` | string，非 null | 地图名。 | demo metadata。 | 必须与 `manifest.mapName` 一致。 |
| `tickrate` | int，非 null | demo tickrate。 | demo metadata。 | 正整数；必须与 `manifest.tickrate` 一致。 |
| `durationSeconds` | number，非 null | 比赛持续秒数。 | 可由 `lastRound.endTick - firstRound.startTick` 除以 tickrate 得出。 | 必须大于 0。 |
| `serverName` | string 或 null，非缺省 | 服务器名称。 | demo metadata；demo 不提供时为 null。 | 字符串，允许 Unicode。 |
| `source` | string，非 null | 数据来源。 | producer 写入。 | 例如 `"demo"`。 |
| `teamA.teamKey` | string，非 null | A 队 key。 | producer team mapping。 | 必须为 `"teamA"`。 |
| `teamA.name` | string 或 null，非缺省 | A 队名称。 | demo/team metadata；demo 不提供时为 null。 | 非空字符串或 null。 |
| `teamA.score` | int，非 null | A 队最终比分。 | 由回合结果汇总。 | `>= 0`。 |
| `teamB.teamKey` | string，非 null | B 队 key。 | producer team mapping。 | 必须为 `"teamB"`。 |
| `teamB.name` | string 或 null，非缺省 | B 队名称。 | demo/team metadata；demo 不提供时为 null。 | 非空字符串或 null。 |
| `teamB.score` | int，非 null | B 队最终比分。 | 由回合结果汇总。 | `>= 0`。 |

## players.json

作用：列出本场比赛所有玩家及其队伍归属。

每一行代表一名玩家。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `steamId64` | string，非 null | 玩家 SteamID。 | demo player info。 | 唯一；必须满足 `^\d{17}$`。 |
| `name` | string，非 null | 玩家游戏内名称。 | demo player info。 | 非空字符串，允许 Unicode。 |
| `teamKey` | string，非 null | 玩家所属队伍。 | producer team mapping。 | 必须为 `teamA` 或 `teamB`。 |

## rounds.json

作用：记录每个正式回合的基础信息。

每一行代表一个正式回合。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 正式回合编号。 | producer 按正式回合顺序编号。 | 从 1 开始，连续递增。 |
| `startTick` | int，非 null | 回合开始 tick。 | parser round start。 | `> 0`。 |
| `freezeEndTick` | int，非 null | freeze time 结束 tick。 | parser freeze end。 | `>= startTick`。 |
| `endTick` | int，非 null | 回合结束 tick。 | parser round end。 | `>= freezeEndTick`。 |
| `teamASide` | side，非 null | A 队本回合阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `teamBSide` | side，非 null | B 队本回合阵营。 | 必须与 `teamASide` 相反。 | `"t"` 或 `"ct"`。 |
| `teamAScoreBefore` | int，非 null | A 队回合开始前比分。 | 由此前回合胜负累计。 | `>= 0`。 |
| `teamBScoreBefore` | int，非 null | B 队回合开始前比分。 | 由此前回合胜负累计。 | `>= 0`。 |
| `teamAEconomy` | team economy type，非 null | A 队经济类型。 | 默认由本回合 5 名队员经济类型多数投票得到；若该队赢下 R1/R13，下一回合标记为 `conversion`。 | `pistol/eco/semi/force/full/conversion`。 |
| `teamBEconomy` | team economy type，非 null | B 队经济类型。 | 同上。 | `pistol/eco/semi/force/full/conversion`。 |
| `winnerTeamKey` | string，非 null | 获胜队伍。 | round outcome。 | 必须为 `teamA` 或 `teamB`。 |
| `winnerSide` | side，非 null | 获胜阵营。 | round outcome。 | `"t"` 或 `"ct"`。 |
| `endReason` | string，非 null | 回合结束原因。 | parser round end reason。 | `t_win/ct_win/target_bombed/bomb_defused/time_ran_out`。 |

## player-economies.json

作用：记录每名玩家每个正式回合的经济状态。

每一行代表一名玩家在一个正式回合的经济快照。理想情况下行数应为：
`rounds.length * players.length`。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 正式回合编号。 | 引用 `rounds.roundNumber`。 | 必须存在于 `rounds.json`。 |
| `steamId64` | string，非 null | 玩家 SteamID。 | 引用 `players.steamId64`。 | 必须存在于 `players.json`。 |
| `teamKey` | string，非 null | 玩家队伍。 | player/team mapping。 | 必须与 `players.teamKey` 一致。 |
| `side` | side，非 null | 玩家本回合阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `startMoney` | int，非 null | 回合开始时金钱。 | buy phase 前玩家金钱。 | `>= 0`，`<= 16000`。 |
| `moneySpent` | int，非 null | 本回合花费。 | buy phase 购买总额。 | `>= 0`，`<= 16000`。 |
| `equipmentValue` | int，非 null | freeze end 时装备价值。 | freeze end 玩家装备总值。 | `>= 0`。 |
| `type` | economy type，非 null | 玩家经济类型。 | 按下方规则计算。 | `pistol/eco/semi/force/full`。 |
| `hasArmor` | bool，非 null | 是否有护甲。 | freeze end 玩家装备状态。 | true/false。 |
| `hasHelmet` | bool，非 null | 是否有头盔。 | freeze end 玩家装备状态。 | true/false。 |
| `hasDefuseKit` | bool，非 null | 是否有拆弹器。 | freeze end 玩家装备状态。 | true/false。 |
| `primaryWeapon` | string 或 null，非缺省 | 主武器。 | freeze end 玩家装备状态。 | 没有主武器时为 null。 |
| `secondaryWeapon` | string 或 null，非缺省 | 副武器。 | freeze end 玩家装备状态。 | 没有副武器时为 null。 |
| `grenadeCount` | int，非 null | 持有投掷物数量。 | freeze end 玩家装备状态。 | `>= 0`。 |

`type` 计算顺序如下，先命中者生效：

1. `pistol`: MR12 的 R1/R13，由回合位置决定；加时局使用加时初始经济，不标记为 pistol。
2. `full`: `equipmentValue >= 4000`。
3. `eco`: `moneySpent < 1000 && equipmentValue < 1000`。
4. `force`: `startMoney > 0 && moneySpent / startMoney >= 0.80`。
5. `semi`: 其他情况。

`conversion` 仅用于 `rounds.json` 的队伍经济：R2/R14 中赢下前一个手枪局
的队伍标记为 `conversion`，用于把手枪局后的经济转换局与普通强起/长枪局分开。

## kills.json

作用：记录正式回合中的击杀事件。raw event 层应保留 self kill、world kill、
bomb death 等事件；它们是否计入 `player-stats.json` 由聚合口径决定。

每一行代表一次击杀。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 事件所属正式回合。 | parser event round。 | 必须存在于 `rounds.json`。 |
| `tick` | int，非 null | 击杀发生 tick。 | parser kill event。 | `> 0`。 |
| `killerSteamId64` | string，可 null | 击杀者 SteamID。 | kill event attacker。 | 玩家击杀应存在于 `players.json`；world/bomb 可 null。 |
| `victimSteamId64` | string，非 null | 死亡者 SteamID。 | kill event victim。 | 必须存在于 `players.json`。 |
| `assisterSteamId64` | string，可 null | 普通助攻者 SteamID。 | kill event assister。 | 若存在，必须存在于 `players.json`。 |
| `flashAssisterSteamId64` | string，可 null | 闪光助攻者 SteamID。 | 由 flash/blind/kill 关联得到。 | `flashAssist=true` 时必须存在；否则为 null。 |
| `killerTeamKey` | string，可 null | 击杀者队伍。 | killer team mapping。 | 应与 killer 的 `players.teamKey` 一致。 |
| `victimTeamKey` | string，非 null | 死亡者队伍。 | victim team mapping。 | 必须与 victim 的 `players.teamKey` 一致。 |
| `killerSide` | side，可 null | 击杀者阵营。 | round side mapping；非玩家 killer 时为 null。 | `"t"` 或 `"ct"`。 |
| `victimSide` | side，非 null | 死亡者阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `weapon` | string，非 null | 击杀武器或死亡原因。 | kill event weapon/cause。 | 例如 `ak47`、`awp`、`planted_c4`、`world`。 |
| `killerActiveWeapon` | string 或 null，非缺省 | killer 击杀瞬间手持武器。 | kill tick 附近玩家状态；非玩家 killer 时为 null。 | 字符串或 null。 |
| `victimActiveWeapon` | string 或 null，非缺省 | victim 死亡瞬间手持武器。 | kill tick 附近玩家状态。 | 用于 weapon duel / AWP duel；不可得时为 null 并由 QA 报告。 |
| `headshot` | bool，非 null | 是否爆头击杀。 | kill event flag。 | true/false。 |
| `flashAssist` | bool，非 null | 是否存在闪光助攻。 | kill event flag。 | true/false。 |
| `tradeKill` | bool，非 null | 该击杀是否为 trade kill。 | 由固定 trade window 规则派生。 | true/false。 |
| `tradeDeath` | bool，非 null | 该死亡是否被队友 trade。 | 由固定 trade window 规则派生。 | true/false。 |
| `throughSmoke` | bool，非 null | 是否穿烟击杀。 | parser/game flag。 | true/false。 |
| `noScope` | bool，非 null | 是否盲狙击杀。 | parser/game flag。 | true/false。 |
| `penetratedObjects` | int，非 null | 子弹穿透物体数量。 | parser penetration count。 | `>= 0`。 |
| `killerPosition` | vec3，可 null | 击杀者位置。 | kill event position；非玩家 killer 时为 null。 | 坐标对象组件必须为 number。 |
| `victimPosition` | vec3，非 null | 死亡者位置。 | kill event position。 | 坐标对象组件必须为 number。 |

## damages.json

作用：记录正式回合中的伤害事件。

每一行代表一次伤害。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 事件所属正式回合。 | parser event round。 | 必须存在于 `rounds.json`。 |
| `tick` | int，非 null | 伤害发生 tick。 | parser damage event。 | `> 0`。 |
| `attackerSteamId64` | string，可 null | 伤害来源玩家。 | damage event attacker。 | 若存在，必须存在于 `players.json`。 |
| `victimSteamId64` | string，非 null | 受害玩家。 | damage event victim。 | 必须存在于 `players.json`。 |
| `attackerTeamKey` | string，可 null | 攻击者队伍。 | attacker team mapping。 | 应与 attacker 的 `players.teamKey` 一致。 |
| `victimTeamKey` | string，非 null | 受害者队伍。 | victim team mapping。 | 必须与 victim 的 `players.teamKey` 一致。 |
| `attackerSide` | side，可 null | 攻击者阵营。 | round side mapping；非玩家伤害来源时为 null。 | `"t"` 或 `"ct"`。 |
| `victimSide` | side，非 null | 受害者阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `weapon` | string，非 null | 造成伤害的武器或来源。 | damage event weapon/cause。 | 例如 `ak47`、`inferno`、`hegrenade`；不得为空字符串。 |
| `hitgroup` | string，非 null | 命中部位。 | damage event hitgroup。 | `head/chest/stomach/left_arm/right_arm/left_leg/right_leg/generic/gear/neck`。 |
| `healthDamage` | int，非 null | 封顶后的有效生命值伤害。 | `min(healthDamageRaw, victimHealthBefore)`；用于 ADR。 | `>= 0`，且 `<= victimHealthBefore`。 |
| `healthDamageRaw` | int，非 null | parser 原始未封顶生命值伤害。 | demo parser damage event。 | `>= healthDamage`。 |
| `armorDamage` | int，非 null | 护甲伤害。 | damage event。 | `>= 0`。 |
| `victimHealthBefore` | int，非 null | 伤害前生命值。 | damage event state。 | `0..100`。 |
| `victimHealthAfter` | int，非 null | 伤害后生命值。 | damage event state。 | `0..100`，应不大于 before。 |
| `victimArmorBefore` | int，非 null | 伤害前护甲值。 | damage event state。 | `0..100`。 |
| `victimArmorAfter` | int，非 null | 伤害后护甲值。 | damage event state。 | `0..100`，应不大于 before。 |
| `attackerPosition` | vec3，可 null | 攻击者位置。 | damage event state；非玩家来源时为 null。 | 坐标对象组件必须为 number。 |
| `victimPosition` | vec3，非 null | 受害者位置。 | damage event state。 | 坐标对象组件必须为 number。 |

## blinds.json

作用：记录闪光弹造成的致盲事件。一颗 flash 可以产生多条 blind row。

每一行代表一名玩家被一次闪光影响。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 事件所属正式回合。 | parser event round。 | 必须存在于 `rounds.json`。 |
| `tick` | int，非 null | 致盲发生 tick。 | blind event。 | `> 0`。 |
| `flashId` | string 或 null，非缺省 | 闪光弹唯一标识。 | grenade/blind 关联；不可得时为 null。 | 同一颗 flash 的 blind rows 共享。 |
| `flasherSteamId64` | string，非 null | 投掷闪光弹的玩家。 | flash thrower。 | 必须存在于 `players.json`。 |
| `flashedSteamId64` | string，非 null | 被闪玩家。 | blind victim。 | 必须存在于 `players.json`。 |
| `flasherTeamKey` | string，非 null | 投掷者队伍。 | team mapping。 | 必须与 flasher 的 `players.teamKey` 一致。 |
| `flashedTeamKey` | string，非 null | 被闪者队伍。 | team mapping。 | 必须与 flashed 的 `players.teamKey` 一致。 |
| `flasherSide` | side，非 null | 投掷者阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `flashedSide` | side，非 null | 被闪者阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `durationSeconds` | number，非 null | 致盲持续秒数。 | parser blind duration。 | `0..6`。 |

## bombs.json

作用：记录炸弹事件。

每一行代表一次炸弹相关事件。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 事件所属正式回合。 | parser event round。 | 必须存在于 `rounds.json`。 |
| `tick` | int，非 null | 事件发生 tick。 | bomb event。 | `> 0`。 |
| `type` | string，非 null | 炸弹事件类型。 | parser bomb event。 | `plant_begin/planted/defuse_begin/defused/exploded/dropped/picked_up`。 |
| `site` | string 或 null，非缺省 | 炸弹点。 | parser bombsite；不适用时为 null。 | `a` 或 `b`；原始数字 id 放入 `siteId`。 |
| `siteId` | string 或 null，非缺省 | parser 原始 bombsite id。 | parser raw site code。 | 例如旧数据中的 `433/434`。 |
| `actorSteamId64` | string，可 null | 事件执行者。 | planter/defuser/dropper/picker。 | plant/defuse 应存在于 `players.json`；explode 可 null。 |
| `actorTeamKey` | string，可 null | 执行者队伍。 | actor team mapping。 | 应与 actor 的 `players.teamKey` 一致。 |
| `actorSide` | side，可 null | 执行者阵营。 | round side mapping；爆炸等无玩家 actor 时为 null。 | plant 为 `t`，defuse 为 `ct`。 |
| `position` | vec3，非 null | 事件位置。 | bomb event position。 | 坐标对象组件必须为 number。 |

## grenades.json

作用：记录投掷物投出和生效位置。

每一行代表一颗投掷物。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 事件所属正式回合。 | parser event round。 | 必须存在于 `rounds.json`。 |
| `grenadeId` | string 或 null，非缺省 | 投掷物唯一标识。 | parser entity id 或 producer 生成；不可得时为 null。 | 用于关联 flash blind、grenade damage、smoke 生命周期。 |
| `throwTick` | int，非 null | 投出 tick。 | grenade throw event。 | `> 0`。 |
| `effectTick` | int，非 null | 生效 tick。 | detonation/activation event。 | `>= throwTick`。 |
| `destroyTick` | int 或 null，非缺省 | 投掷物消失 tick。 | smoke/molotov end event；不适用时为 null。 | 若非 null，必须 `>= effectTick`。 |
| `grenade` | string，非 null | 投掷物类型。 | grenade event。 | `flashbang/smoke/molotov/incendiary/hegrenade/decoy`。 |
| `throwerSteamId64` | string，非 null | 投掷者。 | grenade thrower。 | 必须存在于 `players.json`。 |
| `throwerTeamKey` | string，非 null | 投掷者队伍。 | team mapping。 | 必须与 thrower 的 `players.teamKey` 一致。 |
| `throwerSide` | side，非 null | 投掷者阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `throwPosition` | vec3，非 null | 投出时投掷者位置。 | throw event state。 | 坐标对象组件必须为 number；`{0,0,0}` 不应用作未知。 |
| `effectPosition` | vec3，非 null | 生效位置。 | detonation/activation state。 | 坐标对象组件必须为 number。 |

## clutches.json

作用：记录残局情况。该文件是派生事件，可以从回合内存活状态和击杀事件复算。

每一行代表一次 1vX 残局。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 残局所属正式回合。 | clutch detector。 | 必须存在于 `rounds.json`。 |
| `tick` | int，非 null | 残局开始 tick。 | 某队只剩一名玩家存活时的 tick。 | 必须在该回合 tick 范围内。 |
| `clutcherSteamId64` | string，非 null | 残局玩家。 | clutch detector。 | 必须存在于 `players.json`。 |
| `clutcherTeamKey` | string，非 null | 残局玩家队伍。 | team mapping。 | 必须与 clutcher 的 `players.teamKey` 一致。 |
| `clutcherSide` | side，非 null | 残局玩家阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `opponentCount` | int，非 null | 残局开始时敌方存活人数。 | clutch detector。 | `1..5`。 |
| `won` | bool，非 null | 残局玩家是否赢下回合。 | 比较 `rounds.winnerTeamKey`。 | true/false。 |
| `survived` | bool，非 null | 残局玩家回合结束是否存活。 | round end player state。 | true/false。 |
| `killCount` | int，非 null | 残局开始后该玩家击杀数。 | 从 `kills.json` 统计。 | `0..opponentCount`。 |

## player-stats.json

作用：记录每名玩家整张地图的聚合统计。

每一行代表一名玩家。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `steamId64` | string，非 null | 玩家 SteamID。 | 引用 `players.steamId64`。 | 必须存在于 `players.json`。 |
| `teamKey` | string，非 null | 玩家队伍。 | 引用 `players.teamKey`。 | 必须与 `players.json` 一致。 |
| `rounds` | int，非 null | 玩家参与的正式回合数。 | 必须等于 `rounds.json` 行数。 | `>= 0`。 |
| `kills` | int，非 null | 有效击杀数。 | 只统计正式回合敌方玩家击杀，不计 self/team/world/bomb。 | `>= 0`。 |
| `deaths` | int，非 null | 死亡数。 | 正式回合死亡数；详细分项见 `combatDeathCount`、`bombDeathCount`。 | `>= 0`。 |
| `assists` | int，非 null | 普通助攻数。 | 从 `kills.assisterSteamId64` 聚合。 | `>= 0`。 |
| `damageHealth` | int，非 null | 封顶后的有效生命值伤害总和。 | 聚合 `damages.healthDamage`，不是 raw damage。 | `>= 0`。 |
| `damageArmor` | int，非 null | 护甲伤害总和。 | 从 `damages.armorDamage` 聚合。 | `>= 0`。 |
| `adr` | number，非 null | Average Damage per Round。 | `damageHealth / rounds`；对齐平台/OCR ADR。 | `>= 0`。 |
| `utilityDamage` | int，非 null | 投掷物造成的封顶有效生命值伤害。 | 从 HE、molotov/incendiary、inferno 等 damage 的 `healthDamage` 聚合。 | `>= 0`。 |
| `averageUtilityDamagePerRound` | number，非 null | 每回合平均投掷物伤害。 | `utilityDamage / rounds`。 | `>= 0`。 |
| `headshotCount` | int，非 null | 爆头击杀数。 | `kills.headshot=true` 的有效击杀数。 | `0..kills`。 |
| `firstKillCount` | int，非 null | 首杀回合数。 | 每回合第一条有效敌方击杀归给 killer。 | `0..rounds`。 |
| `firstDeathCount` | int，非 null | 首死回合数。 | 每回合第一条有效敌方击杀归给 victim。 | `0..rounds`。 |
| `tradeKillCount` | int，非 null | trade kill 数。 | `kills.tradeKill=true` 的有效击杀数。 | `>= 0`，`<= kills`。 |
| `tradeDeathCount` | int，非 null | 被 trade 的死亡数。 | `kills.tradeDeath=true` 的死亡数。 | `>= 0`，`<= deaths`。 |
| `kast` | number，非 null | KAST 百分比。 | `kast_rounds / rounds * 100`。 | `[0,100]`。 |
| `oneKillCount` | int，非 null | exactly 1 kill 的回合数。 | 按每回合有效击杀数统计。 | `0..rounds`。 |
| `twoKillCount` | int，非 null | exactly 2 kills 的回合数。 | 同上。 | `0..rounds`。 |
| `threeKillCount` | int，非 null | exactly 3 kills 的回合数。 | 同上。 | `0..rounds`。 |
| `fourKillCount` | int，非 null | exactly 4 kills 的回合数。 | 同上。 | `0..rounds`。 |
| `fiveKillCount` | int，非 null | exactly 5 kills 的回合数。 | 同上。 | `0..rounds`。 |
| `vsOneCount` | int，非 null | 1v1 残局次数。 | 从 `clutches.json` 聚合。 | `>= 0`。 |
| `vsOneWonCount` | int，非 null | 1v1 残局胜利次数。 | `opponentCount=1 && won=true`。 | `0..vsOneCount`。 |
| `vsOneLostCount` | int，非 null | 1v1 残局失败次数。 | `opponentCount=1 && won=false`。 | `0..vsOneCount`。 |
| `vsTwoCount` | int，非 null | 1v2 残局次数。 | 从 `clutches.json` 聚合。 | `>= 0`。 |
| `vsTwoWonCount` | int，非 null | 1v2 残局胜利次数。 | `opponentCount=2 && won=true`。 | `0..vsTwoCount`。 |
| `vsTwoLostCount` | int，非 null | 1v2 残局失败次数。 | `opponentCount=2 && won=false`。 | `0..vsTwoCount`。 |
| `vsThreeCount` | int，非 null | 1v3 残局次数。 | 从 `clutches.json` 聚合。 | `>= 0`。 |
| `vsThreeWonCount` | int，非 null | 1v3 残局胜利次数。 | `opponentCount=3 && won=true`。 | `0..vsThreeCount`。 |
| `vsThreeLostCount` | int，非 null | 1v3 残局失败次数。 | `opponentCount=3 && won=false`。 | `0..vsThreeCount`。 |
| `vsFourCount` | int，非 null | 1v4 残局次数。 | 从 `clutches.json` 聚合。 | `>= 0`。 |
| `vsFourWonCount` | int，非 null | 1v4 残局胜利次数。 | `opponentCount=4 && won=true`。 | `0..vsFourCount`。 |
| `vsFourLostCount` | int，非 null | 1v4 残局失败次数。 | `opponentCount=4 && won=false`。 | `0..vsFourCount`。 |
| `vsFiveCount` | int，非 null | 1v5 残局次数。 | 从 `clutches.json` 聚合。 | `>= 0`。 |
| `vsFiveWonCount` | int，非 null | 1v5 残局胜利次数。 | `opponentCount=5 && won=true`。 | `0..vsFiveCount`。 |
| `vsFiveLostCount` | int，非 null | 1v5 残局失败次数。 | `opponentCount=5 && won=false`。 | `0..vsFiveCount`。 |
| `bombPlantCount` | int，非 null | 成功下包次数。 | 从 `bombs` 中 planted/plant 事件按 actor 聚合。 | `>= 0`。 |
| `bombDefuseCount` | int，非 null | 成功拆包次数。 | 从 `bombs` 中 defused/defuse 事件按 actor 聚合。 | `>= 0`。 |
| `wallbangKillCount` | int，非 null | 穿墙击杀数。 | `penetratedObjects > 0` 的有效击杀数。 | `0..kills`。 |
| `noScopeKillCount` | int，非 null | 盲狙击杀数。 | `kills.noScope=true` 的有效击杀数。 | `0..kills`。 |
| `collateralKillCount` | int，非 null | 一枪多杀相关击杀数。 | 需由 shot/tick/weapon 规则定义。 | `>= 0`。 |
| `kast_rounds` | int，非 null | 满足 KAST 的原始回合数。 | kill、assist、survive、traded 任一满足则该回合计 1。 | `0..rounds`。 |

正式字段：

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `flashAssistCount` | int，非 null | 闪光助攻次数。 | 从 `kills.flashAssisterSteamId64` 聚合。 | `>= 0`。 |
| `enemyFlashDurationSeconds` | number，非 null | 闪到敌人的总秒数。 | 从 `blinds` 聚合，排除队友。 | `>= 0`。 |
| `teamFlashDurationSeconds` | number，非 null | 闪到队友的总秒数。 | 从 `blinds` 聚合，只计队友。 | `>= 0`。 |
| `combatDeathCount` | int，非 null | 战斗死亡数。 | 排除 self/world/bomb 的死亡。 | `>= 0`。 |
| `bombDeathCount` | int，非 null | C4 爆炸死亡数。 | `kills.weapon=planted_c4` 或等价 death cause。 | `>= 0`。 |

## shots.json

作用：记录逐枪事件。该文件可选；启用时必须完整遵守本节字段合同。

每一行代表一次开火。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 开火所属正式回合。 | parser shot event。 | 必须存在于 `rounds.json`。 |
| `tick` | int，非 null | 开火 tick。 | parser shot event。 | `> 0`。 |
| `steamId64` | string，非 null | 开火玩家。 | shot event shooter。 | 必须存在于 `players.json`。 |
| `teamKey` | string，非 null | 开火玩家队伍。 | team mapping。 | 必须与 shooter 的 `players.teamKey` 一致。 |
| `side` | side，非 null | 开火玩家阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `weapon` | string，非 null | 开火武器。 | shot event weapon。 | 非空字符串。 |
| `position` | vec3，非 null | 开火时玩家位置。 | shot event state。 | 坐标对象组件必须为 number。 |
| `velocity` | vec3，非 null | 开火时玩家速度。 | shot event state。 | 坐标对象组件必须为 number。 |
| `yaw` | number，非 null | 开火时视角 yaw。 | shot event state。 | `[-180,180]`。 |
| `pitch` | number，非 null | 开火时视角 pitch。 | shot event state。 | `[-90,90]`。 |

## positions-1s.json

作用：记录每秒玩家状态快照。该文件可选；启用时必须完整遵守本节字段合同。

每一行代表某名玩家在某个采样 tick 的状态。

| 字段 | 类型 / nullable | 含义 | 计算方式 / 来源 | 范围限制 |
|---|---|---|---|---|
| `roundNumber` | int，非 null | 快照所属正式回合。 | sampler。 | 必须存在于 `rounds.json`。 |
| `tick` | int，非 null | 采样 tick。 | sampler。 | `>= 0`。 |
| `steamId64` | string，非 null | 玩家 SteamID。 | player state。 | 必须存在于 `players.json`。 |
| `teamKey` | string，非 null | 玩家队伍。 | team mapping。 | 必须与 `players.teamKey` 一致。 |
| `side` | side，非 null | 玩家阵营。 | round side mapping。 | `"t"` 或 `"ct"`。 |
| `alive` | bool，非 null | 玩家是否存活。 | player state。 | true/false。 |
| `position` | vec3，可 null | 玩家位置。 | player state。 | alive=true 时必须存在。 |
| `yaw` | number，可 null | 玩家视角 yaw。 | player state。 | `[-180,180]`。 |
| `pitch` | number，可 null | 玩家视角 pitch。 | player state。 | `[-90,90]`。 |
| `health` | int，非 null | 生命值。 | player state。 | `0..100`。 |
| `armor` | int，非 null | 护甲值。 | player state。 | `0..100`。 |
| `money` | int，非 null | 玩家金钱。 | player state。 | `0..16000`。 |
| `activeWeapon` | string，可 null | 当前手持武器。 | player state。 | 非空字符串。 |
| `flashDurationRemaining` | number，非 null | 剩余白屏秒数。 | player state。 | `0..6`。 |
| `hasBomb` | bool，非 null | 是否携带 C4。 | player state。 | true/false。 |
| `hasDefuseKit` | bool，非 null | 是否有拆弹器。 | player state。 | true/false。 |
