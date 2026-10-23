# 补枪标记与击杀事件顺序修复

## 判定与修复位置

`python/src/cs2df/parse.py` 的 `parse_demo` 从 `parse_event("player_death")`
取得事件；`_rows` 将 DataFrame 按返回行序转换为记录。旧 `build_kills`
继续沿用该顺序，`_annotate_trades` 却依赖向前遍历时 tick 单调。
它还缺少同回合、非负时间差及敌我关系约束，导致自杀/队杀或跨回合事件被关联。

修复在事件构建层执行：按 `(roundNumber, tick)` 稳定排序，清空派生标记，
在同回合的六秒窗口内寻找最近一条合法敌方击杀；当前击杀者必须与该次死者同队。
窗口由实际 tickrate 换算。相同 tick 保留源序；不会凭空恢复 sub-tick 顺序。

## 上游顺序来源与调查边界

科隆 ZIP 均记录 parser `demoparser2 0.41.3`。核对该版本的公开源码
[`54e320f`](https://github.com/LaihoE/demoparser/tree/54e320f381417b85c2b085921253499bacf9acdc)：
[事件 tick 来自 DEM frame header](https://github.com/LaihoE/demoparser/blob/54e320f381417b85c2b085921253499bacf9acdc/src/parser/src/second_pass/parser.rs#L130-L147)，
[输出合并按片段字节指针排序后拼接](https://github.com/LaihoE/demoparser/blob/54e320f381417b85c2b085921253499bacf9acdc/src/parser/src/parse_demo.rs#L285-L325)，
[Python DataFrame 转换未对行按 tick 排序](https://github.com/LaihoE/demoparser/blob/54e320f381417b85c2b085921253499bacf9acdc/src/python/src/lib.rs#L1017-L1072)。
这条链路没有全局 tick 排序保证。没有发现按 world 武器分流的实现；
合并有字节位置排序，不能把本案归咎于线程完成顺序。

202图中仅上述 Ancient 有两处相邻 tick 回退，均发生在 world 事件之后。
DF的多段合并只对每段做 tick 平移、过滤和顺序拼接，保留单段原序。
原 `.dem`、历史运行二进制及完整 part 清单已不可得，无法唯一证明最初是
恢复录像帧、某段输入还是其他历史来源触发。因此本修复针对可确认的消费端假设错误。
同回合的5次重复 victim 死亡仍是独立的数据来源疑点，排序不会删除这些原始事件。

## 现有 ZIP 的复核

本次只读检查科隆202个 ZIP、28685条击杀事件。修复会改变26图的补枪标记：
33条 `tradeKill`、35条 `tradeDeath` 布尔值变化，两个原始标记总数均从6157变为6124。
下游已排除非敌方击杀，因此下游原补枪计数6137与原始flag总数6157不同。

异常示例为 `s2-r3-m1-m1-de_ancient.zip`：原数组index12是第12回合tick101299
的自杀，index15是第2回合tick18017的敌方击杀，旧逻辑以负差值-83282关联。
原数组中的行序被稳定排序修复；回合/队伍检查独立保证这些事件无法成为补枪。

## 旧资料与发布

重建补枪需要 `kills.json`、`players.json` 及 manifest 的 tickrate，现有ZIP已具备。
DAK可以从这些原始事实重新派生分析，科隆无需重新解析原 `.dem`。
DF修复只影响今后的导出，不会改写已有ZIP。需要改写旧ZIP时，必须同时更新
`player-stats.json`中的补枪和KAST等依赖统计，并保留原始文件及生成来源；
只替换两个flag会造成包内统计不一致。本次没有执行旧ZIP改写。

这次保持既有六秒、最近一条队友死亡的补枪口径，修复实现遗漏；没有新增schema字段。
发布版本与包生成流程独立执行，PR本身不发布。已有网站Evidence与统计投影不会自动更新。
