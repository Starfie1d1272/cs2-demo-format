/**
 * cs2-demo-format — Canonical Zod Schemas (v0.1.0)
 *
 * These schemas define the structure of every JSON file inside a CS2 demo export ZIP.
 * Both the producer (cs2-insight-agent) and consumers (RivalHub, etc.) should validate
 * against these schemas to ensure compatibility.
 *
 * Fields marked [TBD] will be finalized at v1.0.0.
 */

import { z } from "zod";

// ── Shared primitives ──────────────────────────────────────────────────────────

/** 3D world coordinate (CS2 map coordinates) */
export const vec3Schema = z.object({ x: z.number(), y: z.number(), z: z.number() });

/** Which side a player/team is on in a given round */
export const sideSchema = z.enum(["t", "ct", "unknown"]);

/** Team-level economy classification for a round */
export const economyTypeSchema = z.enum(["eco", "force", "semi", "full", "pistol"]);

// Convenience aliases for nullable/optional column types
const nullInt  = z.number().int().nullable().optional();
const nullReal  = z.number().nullable().optional();
const nullStr   = z.string().nullable().optional();
const nullBool  = z.boolean().nullable().optional();
const nullSide  = sideSchema.nullable().optional();
const nullVec3  = vec3Schema.nullable().optional();

// ── manifest.json ─────────────────────────────────────────────────────────────

/**
 * Top-level metadata file.
 * `schemaVersion` will be renamed from "rivalhub-demo-export/1"
 * to "cs2-demo-format/1.0" at v1.0.0.
 */
export const manifestSchema = z.object({
  /** Format identifier + version. Currently "rivalhub-demo-export/1". */
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

// ── players.json ──────────────────────────────────────────────────────────────

/** One entry per player in the match. */
export const playerRowSchema = z.object({
  /** Steam 64-bit ID as a decimal string. */
  steamId64: z.string(),
  /** In-game name at the time of the demo. */
  name: z.string(),
  /** Opaque team identifier (e.g. "A" / "B") consistent within this export. */
  teamKey: z.string(),
});
export const playersSchema = z.array(playerRowSchema);
export type PlayerRow = z.infer<typeof playerRowSchema>;

// ── rounds.json ───────────────────────────────────────────────────────────────

/**
 * Team economy summary for a round.
 * [TBD] Full schema to be confirmed from cs2-insight-agent v1 first export.
 * Expected to contain at minimum: { type: EconomyType, ... }
 */
export const teamEconomySchema = z.unknown(); // [TBD v1.0.0]

/** One entry per round. */
export const roundRowSchema = z.object({
  roundNumber:     z.number().int(),
  startTick:       nullInt,
  freezeEndTick:   nullInt,
  endTick:         nullInt,
  /** Side A is playing as at the start of this round. */
  teamASide:       nullSide,
  teamBSide:       nullSide,
  /** Score before this round begins. */
  teamAScoreBefore: nullInt,
  teamBScoreBefore: nullInt,
  /**
   * Team A's economy classification for this round. [TBD]
   * @see teamEconomySchema
   */
  teamAEconomy:    teamEconomySchema.optional(),
  teamBEconomy:    teamEconomySchema.optional(),
  /** teamKey of the winning team ("A", "B", or null if undecided). */
  winnerTeamKey:   nullStr,
  winnerSide:      nullSide,
  /** How the round ended (e.g. "ct_win_elimination", "t_win_bomb"). */
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
 * - `twoKillCount` etc. count rounds with **exactly** N kills.
 *   Multi-kill aggregates = two + three + four + five.
 */
export const playerStatsRowSchema = z.object({
  steamId64:    z.string(),
  teamKey:      z.string(),

  kills:        nullInt,
  deaths:       nullInt,
  assists:      nullInt,
  damageHealth: nullInt,
  damageArmor:  nullInt,
  /** Average damage per round (round-weighted). */
  adr:          nullReal,
  /** Total utility damage dealt. */
  utilityDamage: nullInt,
  /** Average utility damage per round. */
  averageUtilityDamagePerRound: nullReal,
  headshotCount: nullInt,

  /** Rounds where this player got the first kill of the round. */
  firstKillCount:  nullInt,
  /** Rounds where this player died first. */
  firstDeathCount: nullInt,
  /** Rounds where this player's kill traded an ally's death. */
  tradeKillCount:  nullInt,
  /** Rounds where this player's death was traded by an ally. */
  tradeDeathCount: nullInt,

  /**
   * KAST — percentage of rounds where player had a Kill, Assist, Survived, or was Traded.
   * Value in [0, 100].
   */
  kast: nullReal,

  /** Rounds with exactly 1 kill. */
  oneKillCount:   nullInt,
  /** Rounds with exactly 2 kills. */
  twoKillCount:   nullInt,
  /** Rounds with exactly 3 kills. */
  threeKillCount: nullInt,
  /** Rounds with exactly 4 kills. */
  fourKillCount:  nullInt,
  /** Rounds with exactly 5 kills. */
  fiveKillCount:  nullInt,

  vsOneCount: nullInt, vsOneWonCount: nullInt, vsOneLostCount: nullInt,
  vsTwoCount: nullInt, vsTwoWonCount: nullInt, vsTwoLostCount: nullInt,
  vsThreeCount: nullInt, vsThreeWonCount: nullInt, vsThreeLostCount: nullInt,
  vsFourCount: nullInt, vsFourWonCount: nullInt, vsFourLostCount: nullInt,
  vsFiveCount: nullInt, vsFiveWonCount: nullInt, vsFiveLostCount: nullInt,

  bombPlantedCount:  nullInt,
  bombDefusedCount:  nullInt,
  wallbangKillCount: nullInt,
  noScopeKillCount:  nullInt,
  collateralKillCount: nullInt,
});
export const playerStatsSchema = z.array(playerStatsRowSchema);
export type PlayerStatsRow = z.infer<typeof playerStatsRowSchema>;

// ── player-economies.json ─────────────────────────────────────────────────────

/** Per-player per-round economy snapshot. */
export const playerEconomyRowSchema = z.object({
  roundNumber:     z.number().int(),
  steamId64:       z.string(),
  teamKey:         nullStr,
  side:            nullSide,
  /** Money at round start (before purchases). */
  startMoney:      nullInt,
  /** Total money spent this round. */
  moneySpent:      nullInt,
  /** Equipment value at freeze-time end. */
  equipmentValue:  nullInt,
  /** Economy classification: "eco" | "force" | "semi" | "full" | "pistol" */
  type:            nullStr,
});
export const playerEconomiesSchema = z.array(playerEconomyRowSchema);
export type PlayerEconomyRow = z.infer<typeof playerEconomyRowSchema>;

// ── kills.json ────────────────────────────────────────────────────────────────

/** One entry per kill event. */
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
  /** Grenade type: "flashbang" | "smoke_grenade" | "molotov" | "he_grenade" | "decoy" */
  grenade:            nullStr,
  throwerSteamId64:   nullStr,
  throwerTeamKey:     nullStr,
  throwerSide:        nullSide,
  throwPosition:      nullVec3,
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
