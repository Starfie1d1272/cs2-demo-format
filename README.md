# cs2-demo-format

**CS2 Demo Export Format Specification — v1.3.0**

Defines the ZIP-based export format used to exchange parsed CS2 demo data between tools.
Currently used by [cs2-insight-agent](https://github.com/Starfie1d1272/CS2-insight-agent) (producer)
and [RivalHub](https://github.com/Starfie1d1272/RivalHub) (consumer).

> **Status**: Stable (1.x). Breaking changes require a major version bump.

---

## ZIP Structure

A valid export is a `.zip` file containing the following files:

| File | Required | Description |
|---|---|---|
| `manifest.json` | ✅ | Metadata: schema version, map name, tickrate, file index |
| `match.json` | ✅ | Match-level summary: team names, final scores, duration |
| `players.json` | ✅ | Player list with Steam ID, name, team key |
| `rounds.json` | ✅ | Per-round metadata: sides, scores, economy, outcome |
| `player-stats.json` | ✅ | Per-player aggregated stats (K/D/A, KAST, ADR, clutches…) |
| `player-economies.json` | ✅ | Per-player per-round economy (equipment value, type, spend) |
| `kills.json` | ✅ | Per-kill events with weapon, positions, flags |
| `damages.json` | ✅ | Per-damage events |
| `blinds.json` | ✅ | Flash-blind events |
| `bombs.json` | ✅ | Bomb plant/defuse/explode events |
| `clutches.json` | ✅ | Clutch situations with outcome |
| `grenades.json` | ✅ | Grenade throw/effect events |
| `shots.json` | ⬜ | Shot events (optional, large) |
| `positions-1s.json` | ⬜ | 1-second position snapshots (optional, very large) |

---

## manifest.json

```jsonc
{
  "schemaVersion": "cs2-demo-format/1.0",   // legacy: "rivalhub-demo-export/1" (accepted during transition)
  "exporter": { "name": "cs2-insight-agent", "version": "1.0.0" },
  "parser":   { "name": "cs2-parser", "version": "x.y.z" },
  "demo": { "hash": "<sha256>", "sourceFileName": "match.dem" },
  "mapName": "de_dust2",
  "tickrate": 64,
  "exportedAt": "2026-05-29T10:00:00Z",
  "files": {
    "players":         "players.json",
    "rounds":          "rounds.json",
    "playerStats":     "player-stats.json",
    "playerEconomies": "player-economies.json",
    "kills":           "kills.json",
    "damages":         "damages.json",
    "blinds":          "blinds.json",
    "bombs":           "bombs.json",
    "clutches":        "clutches.json",
    "grenades":        "grenades.json",
    "shots":           "shots.json",
    "positions1s":     "positions-1s.json"
  }
}
```

---

## Field Semantics

### Common types
- **side**: `"t" | "ct" | "unknown"`
- **teamKey**: an opaque string (`"A"` / `"B"` or similar) identifying a team within this map
- **steamId64**: Steam 64-bit ID as a decimal string
- **KAST**: percentage value in range `[0, 100]` (e.g. `73.5`, not `0.735`)
- **economy type**: `"pistol" | "eco" | "semi" | "force" | "full"`

### multiKills / xKillCount
`twoKillCount`, `threeKillCount`, `fourKillCount`, `fiveKillCount` each count **rounds where the player got exactly N kills**.  
Multi-kill aggregates (e.g. "2K and above") = `two + three + four + five`.

### Economy classification algorithm (player-economies.json → `type`)

Evaluated after the buy phase using three per-player inputs:

| Input | Field | Description |
|---|---|---|
| `equipment_value` | `equipmentValue` | Total gear value after purchases |
| `money_spent` | `moneySpent` | Amount spent this round |
| `start_money` | `startMoney` | Money available at round start |

Rules are evaluated in priority order; the first match wins:

| Priority | Type | Condition |
|---|---|---|
| 0 | `pistol` | first round of each half — determined by **round number**, not equipment |
| 1 | `full` | `equipmentValue >= 4000` |
| 2 | `eco` | `moneySpent < 1000` AND `equipmentValue < 2000` |
| 3 | `force` | `startMoney > 0` AND `moneySpent / startMoney > 0.75` |
| 4 | `semi` | everything else (fallback) |

**Price reference**: `full` threshold 4000 = AK (2700) + full armor (1000) + smoke (300).
Survived players carrying full-buy gear are correctly classified as `full` even if they spent nothing.

### teamAEconomy / teamBEconomy (rounds.json)

Team-level classification derived from the 5 player types via **majority vote**.  
Ties resolve conservatively: `eco < semi < force < full` (the lower category wins).  
Currently `null` in exporter output; field is reserved for future use. Consumers must handle `null`.

---

## TypeScript Usage

### Schemas

```ts
import { roundsSchema, playerStatsSchema, killsSchema, SCHEMAS_BY_KEY } from 'cs2-demo-format';
```

See [`schemas/index.ts`](./schemas/index.ts) for all Zod definitions.

### Reference Parser

```ts
import { parseDemoPackage } from 'cs2-demo-format/parser';
import { readFileSync } from 'fs';

const parsed = await parseDemoPackage(readFileSync('export.zip'));
console.log(parsed.manifest.mapName);       // "de_ancient"
console.log(parsed.files.playerStats);      // PlayerStatsRow[]  (warmup filtered)
console.log(parsed.files.match[0].teamA);   // { teamKey, name, score }
```

---

## Validation (Python / language-neutral)

Pre-generated JSON Schema files in [`spec/`](./spec/) allow validation without any Node.js dependency.

```python
import json, jsonschema

# Validate player-stats.json from a real export
data   = json.load(open("player-stats.json"))
schema = json.load(open("spec/playerStats.schema.json"))
jsonschema.validate(data, schema)   # raises ValidationError on mismatch
```

Available schemas: `manifest`, `match`, `players`, `rounds`, `playerStats`,
`playerEconomies`, `kills`, `damages`, `blinds`, `bombs`, `clutches`,
`grenades`, `shots`, `positions1s`.

To regenerate after schema changes:

```bash
pnpm gen:schema
```

---

## Fixtures

[`fixtures/de_ancient-2026-05-17/`](./fixtures/de_ancient-2026-05-17/) contains a real match
export that validates cleanly against all schemas. Use it as:

- **Producer regression test**: your exporter output should match this structure
- **Consumer integration test**: run `pnpm validate:fixtures` to confirm schema ↔ data alignment

```bash
pnpm validate:fixtures   # validate all fixtures against SCHEMAS_BY_KEY
```

---

## Versioning

This package follows [Semantic Versioning](https://semver.org/):
- **0.x.x** — pre-release, schema may change
- **1.0.0** — stable; additive changes are minor bumps, breaking changes are major
- The `schemaVersion` field in `manifest.json` mirrors the major version

## License

MIT
