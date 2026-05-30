/**
 * cs2-demo-format — Canonical Zod Schemas (v1.3.0)
 *
 * These schemas define the structure of every JSON file inside a CS2 demo export ZIP.
 * Both the producer (cs2-insight-agent) and consumers (RivalHub, etc.) should validate
 * against these schemas to ensure compatibility.
 *
 * Schema version string: "cs2-demo-format/1.0" (changed from "rivalhub-demo-export/1").
 * Producers should emit the new string; consumers accept both for a grace period.
 *
 * v1.3.0 additions:
 *   - matchSchema (match.json — match-level summary: teams, scores, duration)
 *   - match added to SCHEMAS_BY_KEY
 */

import { z } from "zod";

// ── Shared primitives ──────────────────────────────────────────────────────────

/**
 * 3D world coordinate (CS2 map coordinates).
 * Components are nullable: the exporter may emit NaN (serialized as null after
 * sanitization) when position tracking is unavailable for a given event.
 */
export const vec3Schema = z.object({
  x: z.number().nullable(),
  y: z.number().nullable(),
  z: z.number().nullable(),
});

/** Which side a player/team is on in a given round */
export const sideSchema = z.enum(["t", "ct", "unknown"]);

/**
 * Per-player economy classification for a round.
 *
 * Priority order (highest wins):
 *   0. `pistol` — first round of each half (round 1 and the opening round of the
 *                 second half / overtime halves); determined by round number, NOT
 *                 by equipment or money values
 *   1. `full`   — equipment_value >= 4000  (AK + armor + util baseline)
 *   2. `eco`    — money_spent < 1000 AND equipment_value < 2000
 *   3. `force`  — start_money > 0 AND money_spent / start_money > 0.75
 *   4. `semi`   — everything else (fallback)
 *
 * Edge cases handled correctly:
 *   - Survived full-buy player who spent nothing → `full` (equip >= 4000)
 *   - Survived rifle/SMG player who saved → `semi`
 *   - Pure save (no spend, pistol-level gear) → `eco`
 */
export const economyTypeSchema = z.enum(["pistol", "eco", "semi", "force", "full"]);

// Convenience aliases for nullable/optional column types
const nullInt      = z.number().int().nullable().optional();
const nullNonNegInt = z.number().int().min(0).nullable().optional();
const nullReal     = z.number().nullable().optional();
const nullNonNeg   = z.number().min(0).nullable().optional();
const nullStr      = z.string().nullable().optional();
const nullBool     = z.boolean().nullable().optional();
const nullSide     = sideSchema.nullable().optional();
const nullVec3     = vec3Schema.nullable().optional();

// ── manifest.json ─────────────────────────────────────────────────────────────

/**
 * Top-level metadata file.
 * `schemaVersion` is now "cs2-demo-format/1.0".
 * Consumers should also accept the legacy "rivalhub-demo-export/1" string.
 */
export const manifestSchema = z.object({
  /** Format identifier + version. "cs2-demo-format/1.0" (legacy: "rivalhub-demo-export/1"). */
  schemaVersion: z.string(),
  /** Tool that produced this export. */
  exporter: z.object({ name: z.string(), version: z.string() }).optional(),
  /** Underlying demo parser used. */
  parser: z.object({ name: z.string(), version: z.string() }).optional(),
  /** Source demo file info. */
  demo: z.object({
    hash: z.string(),
    sourceFileName: z.string().optional(),
  }).optional(),
  mapName: z.string(),
  /** Server tickrate (typically 64 or 128). */
  tickrate: z.number().int(),
  /** ISO 8601 timestamp of when the export was generated. */
  exportedAt: z.string().optional(),
  /** Map from logical file key to filename inside the ZIP. */
  files: z.record(z.string()),
});
export type Manifest = z.infer<typeof manifestSchema>;

// ── match.json ────────────────────────────────────────────────────────────────

/**
 * Match-level summary (single object, not an array).
 * Contains team names, final scores, map, and duration.
 */
export const teamSummarySchema = z.object({
  teamKey: z.string(),
  name: z.string(),
  score: z.number().int(),
});

export const matchSchema = z.object({
  mapName: z.string().optional(),
  tickrate: z.number().int().optional(),
  /** Total match duration in seconds. */
  durationSeconds: z.number().optional(),
  serverName: z.string().optional(),
  /** Source of the data, e.g. "demo". */
  source: z.string().optional(),
  teamA: teamSummarySchema.optional(),
  teamB: teamSummarySchema.optional(),
});
export type TeamSummary = z.infer<typeof teamSummarySchema>;
export type Match = z.infer<typeof matchSchema>;

// ── players.json ──────────────────────────────────────────────────────────────

/** One entry per player in the match. */
export const playerRowSchema = z.object({
  /** Steam 64-bit ID as a decimal string. */
  steamId64: z.string(),
  /** In-game name at the time of the demo. */
  name: z.string(),
  /** Opaque team identifier ("teamA" / "teamB") consistent within this export. */
  teamKey: z.string(),
});
export const playersSchema = z.array(playerRowSchema);
export type PlayerRow = z.infer<typeof playerRowSchema>;

// ── rounds.json ───────────────────────────────────────────────────────────────

/**
 * Team-level economy classification for a round.
 * Derived from the 5 player classifications via majority vote.
 * Tie-break is conservative: eco < semi < force < full.
 * Currently null (not yet emitted by cs2-insight-agent); reserved for future use.
 * Consumers must handle null gracefully.
 */
export const teamEconomySchema = economyTypeSchema.nullable();

/** One entry per round (roundNumber ≥ 1; warmup round 0 is excluded). */
export const roundRowSchema = z.object({
  roundNumber:     z.number().int(),
  startTick:       nullInt,
  freezeEndTick:   nullInt,
  endTick:         nullInt,
  /** Side teamA is playing as at the start of this round. */
  teamASide:       nullSide,
  teamBSide:       nullSide,
  /** Score before this round begins. */
  teamAScoreBefore: nullInt,
  teamBScoreBefore: nullInt,
  /**
   * Team-level economy summary. Currently null; reserved for future use.
   * @see teamEconomySchema
   */
  teamAEconomy:    teamEconomySchema.optional(),
  teamBEconomy:    teamEconomySchema.optional(),
  /** teamKey of the winning team ("teamA", "teamB", or null if undecided). */
  winnerTeamKey:   nullStr,
  winnerSide:      nullSide,
  /** How the round ended (e.g. "ct_win", "t_win", "ct_win_bomb_defused"). */
  endReason:       nullStr,
});
export const roundsSchema = z.array(roundRowSchema);
export type RoundRow = z.infer<typeof roundRowSchema>;

// ── player-stats.json ─────────────────────────────────────────────────────────

/**
 * Per-player aggregated stats for the entire map.
 * One entry per player.
 *
 * Notes:
 * - `kast` is a PERCENTAGE in [0, 100] (e.g. 73.5, not 0.735).
 * - `rounds` = total map rounds (added in exporter v2.1.2+; null in older exports).
 * - `twoKillCount` etc. count rounds with **exactly** N kills.
 *   Multi-kill aggregates = two + three + four + five.
 */
export const playerStatsRowSchema = z.object({
  steamId64:    z.string(),
  teamKey:      z.string(),

  /** Total map rounds played. Added in exporter v2.1.2+; null in older exports. */
  rounds:       nullNonNegInt,

  kills:        nullNonNegInt,
  deaths:       nullNonNegInt,
  assists:      nullNonNegInt,
  damageHealth: nullNonNegInt,
  damageArmor:  nullNonNegInt,
  /** Average damage per round (round-weighted). */
  adr:          nullNonNeg,
  /** Total utility damage dealt. */
  utilityDamage: nullNonNegInt,
  /** Average utility damage per round. */
  averageUtilityDamagePerRound: nullNonNeg,
  headshotCount: nullNonNegInt,

  /** Rounds where this player got the first kill of the round. */
  firstKillCount:  nullNonNegInt,
  /** Rounds where this player died first. */
  firstDeathCount: nullNonNegInt,
  /** Rounds where this player's kill traded an ally's death. */
  tradeKillCount:  nullNonNegInt,
  /** Rounds where this player's death was traded by an ally. */
  tradeDeathCount: nullNonNegInt,

  /**
   * KAST — percentage of rounds where player had a Kill, Assist, Survived, or was Traded.
   * Value in [0, 100].
   */
  kast: z.number().min(0).max(100).nullable().optional(),

  /** Rounds with exactly 1 kill. */
  oneKillCount:   nullNonNegInt,
  /** Rounds with exactly 2 kills. */
  twoKillCount:   nullNonNegInt,
  /** Rounds with exactly 3 kills. */
  threeKillCount: nullNonNegInt,
  /** Rounds with exactly 4 kills. */
  fourKillCount:  nullNonNegInt,
  /** Rounds with exactly 5 kills. */
  fiveKillCount:  nullNonNegInt,

  vsOneCount: nullNonNegInt, vsOneWonCount: nullNonNegInt, vsOneLostCount: nullNonNegInt,
  vsTwoCount: nullNonNegInt, vsTwoWonCount: nullNonNegInt, vsTwoLostCount: nullNonNegInt,
  vsThreeCount: nullNonNegInt, vsThreeWonCount: nullNonNegInt, vsThreeLostCount: nullNonNegInt,
  vsFourCount: nullNonNegInt, vsFourWonCount: nullNonNegInt, vsFourLostCount: nullNonNegInt,
  vsFiveCount: nullNonNegInt, vsFiveWonCount: nullNonNegInt, vsFiveLostCount: nullNonNegInt,

  bombPlantCount:    nullNonNegInt,
  bombDefuseCount:   nullNonNegInt,
  wallbangKillCount: nullNonNegInt,
  noScopeKillCount:  nullNonNegInt,
  collateralKillCount: nullNonNegInt,
  /**
   * Raw count of rounds where the player had a Kill, Assist, Survived, or was Traded.
   * Complementary to `kast` (percentage). kast = kast_rounds / rounds * 100.
   */
  kast_rounds: nullNonNegInt,
});
export const playerStatsSchema = z.array(playerStatsRowSchema);
export type PlayerStatsRow = z.infer<typeof playerStatsRowSchema>;

// ── player-economies.json ─────────────────────────────────────────────────────

/** Per-player per-round economy snapshot. One entry per player per round. */
export const playerEconomyRowSchema = z.object({
  roundNumber:     z.number().int(),
  steamId64:       z.string(),
  teamKey:         nullStr,
  side:            nullSide,
  /** Money at round start (before purchases). */
  startMoney:      nullNonNegInt,
  /** Total money spent this round. */
  moneySpent:      nullNonNegInt,
  /** Equipment value at freeze-time end. */
  equipmentValue:  nullNonNegInt,
  /**
   * Economy classification for this player this round.
   * "full_buy" | "force" | "eco" | "pistol"
   */
  type:            economyTypeSchema.nullable().optional(),
});
export const playerEconomiesSchema = z.array(playerEconomyRowSchema);
export type PlayerEconomyRow = z.infer<typeof playerEconomyRowSchema>;

// ── kills.json ────────────────────────────────────────────────────────────────

/** One entry per kill event (roundNumber ≥ 1; warmup kills excluded). */
export const killRowSchema = z.object({
  roundNumber:         z.number().int(),
  tick:                z.number().int(),
  killerSteamId64:     nullStr,
  victimSteamId64:     nullStr,
  assisterSteamId64:   nullStr,
  killerTeamKey:       nullStr,
  victimTeamKey:       nullStr,
  killerSide:          nullSide,
  victimSide:          nullSide,
  weapon:              nullStr,
  headshot:            nullBool,
  flashAssist:         nullBool,
  /** Killer's death was traded (killed someone who had just killed a teammate). */
  tradeKill:           nullBool,
  /** Victim's death was traded by their team. */
  tradeDeath:          nullBool,
  throughSmoke:        nullBool,
  noScope:             nullBool,
  /** Number of objects (walls) the bullet penetrated. */
  penetratedObjects:   nullInt,
  killerPosition:      nullVec3,
  victimPosition:      nullVec3,
});
export const killsSchema = z.array(killRowSchema);
export type KillRow = z.infer<typeof killRowSchema>;

// ── damages.json ──────────────────────────────────────────────────────────────

/** One entry per damage event. */
export const damageRowSchema = z.object({
  roundNumber:        z.number().int(),
  tick:               z.number().int(),
  attackerSteamId64:  nullStr,
  victimSteamId64:    nullStr,
  attackerTeamKey:    nullStr,
  victimTeamKey:      nullStr,
  attackerSide:       nullSide,
  victimSide:         nullSide,
  weapon:             nullStr,
  hitgroup:           nullStr,
  healthDamage:       nullInt,
  armorDamage:        nullInt,
  victimHealthBefore: nullInt,
  victimHealthAfter:  nullInt,
  victimArmorBefore:  nullInt,
  victimArmorAfter:   nullInt,
});
export const damagesSchema = z.array(damageRowSchema);
export type DamageRow = z.infer<typeof damageRowSchema>;

// ── blinds.json ───────────────────────────────────────────────────────────────

/** One entry per flash-blind event. */
export const blindRowSchema = z.object({
  roundNumber:        z.number().int(),
  tick:               z.number().int(),
  flasherSteamId64:   nullStr,
  flashedSteamId64:   nullStr,
  flasherTeamKey:     nullStr,
  flashedTeamKey:     nullStr,
  flasherSide:        nullSide,
  flashedSide:        nullSide,
  /** Duration the victim was blinded, in seconds. */
  durationSeconds:    nullReal,
});
export const blindsSchema = z.array(blindRowSchema);
export type BlindRow = z.infer<typeof blindRowSchema>;

// ── bombs.json ────────────────────────────────────────────────────────────────

/** One entry per bomb event (plant / defuse / explode / pickup / drop). */
export const bombRowSchema = z.object({
  roundNumber:    z.number().int(),
  tick:           z.number().int(),
  /** Event type, e.g. "plant_begin", "planted", "defuse_begin", "defused", "exploded". */
  type:           nullStr,
  /** Bomb site: "a" | "b" | null */
  site:           nullStr,
  actorSteamId64: nullStr,
  actorTeamKey:   nullStr,
  actorSide:      nullSide,
  position:       nullVec3,
});
export const bombsSchema = z.array(bombRowSchema);
export type BombRow = z.infer<typeof bombRowSchema>;

// ── clutches.json ─────────────────────────────────────────────────────────────

/** One entry per clutch situation. */
export const clutchRowSchema = z.object({
  roundNumber:        z.number().int(),
  tick:               nullInt,
  clutcherSteamId64:  nullStr,
  clutcherTeamKey:    nullStr,
  clutcherSide:       nullSide,
  /** Number of opponents alive when the clutch started (1–5). */
  opponentCount:      nullInt,
  /** Whether the clutcher won the round. */
  won:                nullBool,
  /** Whether the clutcher survived at round end. */
  survived:           nullBool,
  /** Number of kills the clutcher got during the clutch. */
  killCount:          nullInt,
});
export const clutchesSchema = z.array(clutchRowSchema);
export type ClutchRow = z.infer<typeof clutchRowSchema>;

// ── grenades.json ─────────────────────────────────────────────────────────────

/** One entry per grenade throw event. */
export const grenadeRowSchema = z.object({
  roundNumber:        z.number().int(),
  throwTick:          nullInt,
  effectTick:         nullInt,
  /** Grenade type: "flashbang" | "smoke" | "molotov" | "hegrenade" | "decoy" */
  grenade:            nullStr,
  throwerSteamId64:   nullStr,
  throwerTeamKey:     nullStr,
  throwerSide:        nullSide,
  /**
   * Position of the thrower at throw time.
   * NOTE: Currently {0,0,0} for most grenades — exporter limitation.
   * Use `effectPosition` (detonation/activation point) which is always correct.
   */
  throwPosition:      nullVec3,
  /** Detonation / activation position. Always populated. */
  effectPosition:     nullVec3,
});
export const grenadesSchema = z.array(grenadeRowSchema);
export type GrenadeRow = z.infer<typeof grenadeRowSchema>;

// ── shots.json ────────────────────────────────────────────────────────────────

/** One entry per shot fired (optional file — may be very large). */
export const shotRowSchema = z.object({
  roundNumber: z.number().int(),
  tick:        z.number().int(),
  steamId64:   nullStr,
  teamKey:     nullStr,
  side:        nullSide,
  weapon:      nullStr,
  position:    nullVec3,
  velocity:    nullVec3,
  yaw:         nullReal,
  pitch:       nullReal,
});
export const shotsSchema = z.array(shotRowSchema);
export type ShotRow = z.infer<typeof shotRowSchema>;

// ── positions-1s.json ─────────────────────────────────────────────────────────

/** 1-second position snapshots per player (optional file — very large). */
export const positionRowSchema = z.object({
  roundNumber:             z.number().int(),
  tick:                    z.number().int(),
  steamId64:               z.string(),
  teamKey:                 nullStr,
  side:                    nullSide,
  alive:                   nullBool,
  position:                nullVec3,
  yaw:                     nullReal,
  pitch:                   nullReal,
  health:                  nullInt,
  armor:                   nullInt,
  money:                   nullInt,
  activeWeapon:            nullStr,
  flashDurationRemaining:  nullReal,
  hasBomb:                 nullBool,
  hasDefuseKit:            nullBool,
});
export const positionsSchema = z.array(positionRowSchema);
export type PositionRow = z.infer<typeof positionRowSchema>;

// ── All schemas by manifest file key ─────────────────────────────────────────

export const SCHEMAS_BY_KEY = {
  match:           matchSchema,
  players:         playersSchema,
  rounds:          roundsSchema,
  playerStats:     playerStatsSchema,
  playerEconomies: playerEconomiesSchema,
  kills:           killsSchema,
  damages:         damagesSchema,
  blinds:          blindsSchema,
  bombs:           bombsSchema,
  clutches:        clutchesSchema,
  grenades:        grenadesSchema,
  shots:           shotsSchema,
  positions1s:     positionsSchema,
} as const;

/** @deprecated Use SCHEMAS_BY_KEY instead. */
export { SCHEMAS_BY_KEY as FILE_SCHEMAS };
