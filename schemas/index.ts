/**
 * cs2-demo-format — Canonical Zod Schemas (v2.0.0)
 *
 * Strict export contract for CS2 demo ZIP packages.
 * `schemas/index.ts` is the single source of truth; JSON Schema files in
 * `spec/` are generated from these definitions.
 *
 * Schema version string: "cs2-demo-format/2.0".
 */

import { z } from "zod";

// ── Shared primitives ──────────────────────────────────────────────────────────

export const steamId64Schema = z.string().regex(/^\d{17}$/);
export const teamKeySchema = z.enum(["teamA", "teamB"]);
export const sideSchema = z.enum(["t", "ct"]);
export const economyTypeSchema = z.enum(["pistol", "eco", "semi", "force", "full"]);
export const teamEconomyTypeSchema = z.enum(["pistol", "eco", "semi", "force", "full", "conversion"]);
export const endReasonSchema = z.enum([
  "t_win",
  "ct_win",
  "target_bombed",
  "bomb_defused",
  "time_ran_out",
]);
export const bombEventTypeSchema = z.enum([
  "plant_begin",
  "planted",
  "defuse_begin",
  "defused",
  "exploded",
  "dropped",
  "picked_up",
]);
export const grenadeTypeSchema = z.enum([
  "flashbang",
  "smoke",
  "molotov",
  "incendiary",
  "hegrenade",
  "decoy",
]);
export const hitgroupSchema = z.enum([
  "generic",
  "head",
  "chest",
  "stomach",
  "left_arm",
  "right_arm",
  "left_leg",
  "right_leg",
  "gear",
  "neck",
]);

export const vec3Schema = z.object({
  x: z.number(),
  y: z.number(),
  z: z.number(),
}).strict();

const nonNegInt = z.number().int().min(0);
const positiveInt = z.number().int().min(1);
const nonNegNumber = z.number().min(0);
const percentage = z.number().min(0).max(100);
const weaponString = z.string().min(1);
const nullableString = z.string().nullable();
const nullableSteamId64 = steamId64Schema.nullable();
const nullableTeamKey = teamKeySchema.nullable();
const nullableSide = sideSchema.nullable();
const nullableVec3 = vec3Schema.nullable();

// ── manifest.json ─────────────────────────────────────────────────────────────

export const manifestSchema = z.object({
  schemaVersion: z.literal("cs2-demo-format/2.0"),
  exporter: z.object({
    name: z.string().min(1),
    version: z.string().min(1),
  }).strict(),
  parser: z.object({
    name: z.string().min(1),
    version: z.string().min(1),
  }).strict(),
  demo: z.object({
    hash: z.string().regex(/^[a-fA-F0-9]{64}$/).nullable(),
    sourceFileName: nullableString,
  }).strict(),
  mapName: z.string().min(1),
  tickrate: positiveInt,
  exportedAt: z.string().min(1),
  files: z.object({
    match: z.string().min(1),
    players: z.string().min(1),
    rounds: z.string().min(1),
    playerStats: z.string().min(1),
    playerEconomies: z.string().min(1),
    kills: z.string().min(1),
    damages: z.string().min(1),
    blinds: z.string().min(1),
    bombs: z.string().min(1),
    grenades: z.string().min(1),
    clutches: z.string().min(1),
    shots: z.string().min(1).optional(),
    positions1s: z.string().min(1).optional(),
    replay: z.string().min(1).optional(),
  }).strict(),
}).strict();
export type Manifest = z.infer<typeof manifestSchema>;

// ── match.json ────────────────────────────────────────────────────────────────

export const teamSummarySchema = z.object({
  teamKey: teamKeySchema,
  name: nullableString,
  score: nonNegInt,
}).strict();

export const matchSchema = z.object({
  mapName: z.string().min(1),
  tickrate: positiveInt,
  durationSeconds: z.number().positive(),
  serverName: nullableString,
  source: z.string().min(1),
  teamA: teamSummarySchema,
  teamB: teamSummarySchema,
}).strict();
export type TeamSummary = z.infer<typeof teamSummarySchema>;
export type Match = z.infer<typeof matchSchema>;

// ── players.json ──────────────────────────────────────────────────────────────

export const playerRowSchema = z.object({
  steamId64: steamId64Schema,
  name: z.string().min(1),
  teamKey: teamKeySchema,
}).strict();
export const playersSchema = z.array(playerRowSchema);
export type PlayerRow = z.infer<typeof playerRowSchema>;

// ── rounds.json ───────────────────────────────────────────────────────────────

export const teamEconomySchema = teamEconomyTypeSchema;

export const roundRowSchema = z.object({
  roundNumber: positiveInt,
  startTick: positiveInt,
  freezeEndTick: positiveInt,
  endTick: positiveInt,
  teamASide: sideSchema,
  teamBSide: sideSchema,
  teamAScoreBefore: nonNegInt,
  teamBScoreBefore: nonNegInt,
  teamAEconomy: teamEconomySchema,
  teamBEconomy: teamEconomySchema,
  winnerTeamKey: teamKeySchema,
  winnerSide: sideSchema,
  endReason: endReasonSchema,
}).strict();
export const roundsSchema = z.array(roundRowSchema);
export type RoundRow = z.infer<typeof roundRowSchema>;

// ── player-stats.json ─────────────────────────────────────────────────────────

export const playerStatsRowSchema = z.object({
  steamId64: steamId64Schema,
  teamKey: teamKeySchema,
  rounds: nonNegInt,
  kills: nonNegInt,
  deaths: nonNegInt,
  assists: nonNegInt,
  damageHealth: nonNegInt,
  damageArmor: nonNegInt,
  adr: nonNegNumber,
  utilityDamage: nonNegInt,
  averageUtilityDamagePerRound: nonNegNumber,
  headshotCount: nonNegInt,
  firstKillCount: nonNegInt,
  firstDeathCount: nonNegInt,
  tradeKillCount: nonNegInt,
  tradeDeathCount: nonNegInt,
  kast: percentage,
  oneKillCount: nonNegInt,
  twoKillCount: nonNegInt,
  threeKillCount: nonNegInt,
  fourKillCount: nonNegInt,
  fiveKillCount: nonNegInt,
  vsOneCount: nonNegInt,
  vsOneWonCount: nonNegInt,
  vsOneLostCount: nonNegInt,
  vsTwoCount: nonNegInt,
  vsTwoWonCount: nonNegInt,
  vsTwoLostCount: nonNegInt,
  vsThreeCount: nonNegInt,
  vsThreeWonCount: nonNegInt,
  vsThreeLostCount: nonNegInt,
  vsFourCount: nonNegInt,
  vsFourWonCount: nonNegInt,
  vsFourLostCount: nonNegInt,
  vsFiveCount: nonNegInt,
  vsFiveWonCount: nonNegInt,
  vsFiveLostCount: nonNegInt,
  bombPlantCount: nonNegInt,
  bombDefuseCount: nonNegInt,
  wallbangKillCount: nonNegInt,
  noScopeKillCount: nonNegInt,
  collateralKillCount: nonNegInt,
  kast_rounds: nonNegInt,
  flashAssistCount: nonNegInt,
  enemyFlashDurationSeconds: nonNegNumber,
  teamFlashDurationSeconds: nonNegNumber,
  combatDeathCount: nonNegInt,
  bombDeathCount: nonNegInt,
}).strict();
export const playerStatsSchema = z.array(playerStatsRowSchema);
export type PlayerStatsRow = z.infer<typeof playerStatsRowSchema>;

// ── player-economies.json ─────────────────────────────────────────────────────

export const playerEconomyRowSchema = z.object({
  roundNumber: positiveInt,
  steamId64: steamId64Schema,
  teamKey: teamKeySchema,
  side: sideSchema,
  startMoney: nonNegInt,
  moneySpent: nonNegInt,
  equipmentValue: nonNegInt,
  type: economyTypeSchema,
  hasArmor: z.boolean(),
  hasHelmet: z.boolean(),
  hasDefuseKit: z.boolean(),
  primaryWeapon: nullableString,
  secondaryWeapon: nullableString,
  grenadeCount: nonNegInt,
}).strict();
export const playerEconomiesSchema = z.array(playerEconomyRowSchema);
export type PlayerEconomyRow = z.infer<typeof playerEconomyRowSchema>;

// ── kills.json ────────────────────────────────────────────────────────────────

export const killRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  killerSteamId64: nullableSteamId64,
  victimSteamId64: steamId64Schema,
  assisterSteamId64: nullableSteamId64,
  flashAssisterSteamId64: nullableSteamId64,
  killerTeamKey: nullableTeamKey,
  victimTeamKey: teamKeySchema,
  killerSide: nullableSide,
  victimSide: sideSchema,
  weapon: weaponString,
  killerActiveWeapon: nullableString,
  victimActiveWeapon: nullableString,
  headshot: z.boolean(),
  flashAssist: z.boolean(),
  tradeKill: z.boolean(),
  tradeDeath: z.boolean(),
  throughSmoke: z.boolean(),
  noScope: z.boolean(),
  penetratedObjects: nonNegInt,
  killerPosition: nullableVec3,
  victimPosition: vec3Schema,
}).strict();
export const killsSchema = z.array(killRowSchema);
export type KillRow = z.infer<typeof killRowSchema>;

// ── damages.json ──────────────────────────────────────────────────────────────

export const damageRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  attackerSteamId64: nullableSteamId64,
  victimSteamId64: steamId64Schema,
  attackerTeamKey: nullableTeamKey,
  victimTeamKey: teamKeySchema,
  attackerSide: nullableSide,
  victimSide: sideSchema,
  weapon: weaponString,
  hitgroup: hitgroupSchema,
  /**
   * Effective health damage capped by victimHealthBefore.
   * This is the value used by platform ADR and playerStats.damageHealth.
   */
  healthDamage: nonNegInt,
  /** Raw parser health damage before victim-health capping. */
  healthDamageRaw: nonNegInt,
  armorDamage: nonNegInt,
  victimHealthBefore: nonNegInt.max(100),
  victimHealthAfter: nonNegInt.max(100),
  victimArmorBefore: nonNegInt.max(100),
  victimArmorAfter: nonNegInt.max(100),
  attackerPosition: nullableVec3,
  victimPosition: vec3Schema,
}).strict();
export const damagesSchema = z.array(damageRowSchema);
export type DamageRow = z.infer<typeof damageRowSchema>;

// ── blinds.json ───────────────────────────────────────────────────────────────

export const blindRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  flashId: nullableString,
  flasherSteamId64: steamId64Schema,
  flashedSteamId64: steamId64Schema,
  flasherTeamKey: teamKeySchema,
  flashedTeamKey: teamKeySchema,
  flasherSide: sideSchema,
  flashedSide: sideSchema,
  durationSeconds: nonNegNumber.max(6),
}).strict();
export const blindsSchema = z.array(blindRowSchema);
export type BlindRow = z.infer<typeof blindRowSchema>;

// ── bombs.json ────────────────────────────────────────────────────────────────

export const bombRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  type: bombEventTypeSchema,
  site: z.enum(["a", "b"]).nullable(),
  siteId: nullableString,
  actorSteamId64: nullableSteamId64,
  actorTeamKey: nullableTeamKey,
  actorSide: nullableSide,
  position: vec3Schema,
}).strict();
export const bombsSchema = z.array(bombRowSchema);
export type BombRow = z.infer<typeof bombRowSchema>;

// ── clutches.json ─────────────────────────────────────────────────────────────

export const clutchRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  clutcherSteamId64: steamId64Schema,
  clutcherTeamKey: teamKeySchema,
  clutcherSide: sideSchema,
  opponentCount: z.number().int().min(1).max(5),
  won: z.boolean(),
  survived: z.boolean(),
  killCount: nonNegInt.max(5),
}).strict();
export const clutchesSchema = z.array(clutchRowSchema);
export type ClutchRow = z.infer<typeof clutchRowSchema>;

// ── grenades.json ─────────────────────────────────────────────────────────────

export const grenadeRowSchema = z.object({
  roundNumber: positiveInt,
  grenadeId: nullableString,
  throwTick: positiveInt,
  effectTick: positiveInt,
  destroyTick: positiveInt.nullable(),
  grenade: grenadeTypeSchema,
  throwerSteamId64: steamId64Schema,
  throwerTeamKey: teamKeySchema,
  throwerSide: sideSchema,
  throwPosition: vec3Schema,
  effectPosition: vec3Schema,
}).strict();
export const grenadesSchema = z.array(grenadeRowSchema);
export type GrenadeRow = z.infer<typeof grenadeRowSchema>;

// ── shots.json ────────────────────────────────────────────────────────────────

export const shotRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  steamId64: steamId64Schema,
  teamKey: teamKeySchema,
  side: sideSchema,
  weapon: weaponString,
  position: vec3Schema,
  velocity: vec3Schema,
  yaw: z.number().min(-180).max(180),
  pitch: z.number().min(-90).max(90),
}).strict();
export const shotsSchema = z.array(shotRowSchema);
export type ShotRow = z.infer<typeof shotRowSchema>;

// ── positions-1s.json ─────────────────────────────────────────────────────────

export const positionRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  steamId64: steamId64Schema,
  teamKey: teamKeySchema,
  side: sideSchema,
  alive: z.boolean(),
  position: nullableVec3,
  yaw: z.number().min(-180).max(180).nullable(),
  pitch: z.number().min(-90).max(90).nullable(),
  health: nonNegInt.max(100),
  armor: nonNegInt.max(100),
  money: nonNegInt.max(16000),
  activeWeapon: nullableString,
  flashDurationRemaining: nonNegNumber.max(6),
  hasBomb: z.boolean(),
  hasDefuseKit: z.boolean(),
}).strict();
export const positionsSchema = z.array(positionRowSchema);
export type PositionRow = z.infer<typeof positionRowSchema>;

// ── replay.json (compact 2D-replay stream) ────────────────────────────────────
//
// A columnar, quantized player-movement stream tuned for a 2D replay viewer —
// distinct from positions-1s (which stays at 1 Hz for analytics/heatmaps).
//
// Per round, each player carries parallel arrays of length `frameCount`. The
// tick of frame `i` is `startTick + i * tickStep`, so per-frame tick/round are
// implied, not stored. Coordinates are integers in game units divided by
// `meta.coordScale` (1 = raw rounded). Static identity (steamId64/teamKey/side)
// is stored once per player per round, never per frame.
export const replayPlayerTrackSchema = z.object({
  steamId64: steamId64Schema,
  teamKey: teamKeySchema,
  side: sideSchema,
  x: z.array(z.number().int()),
  y: z.array(z.number().int()),
  z: z.array(z.number().int()),
  // facing angle in whole degrees, -180..180
  yaw: z.array(z.number().int().min(-180).max(180)),
  hp: z.array(nonNegInt.max(100)),
  // index into `weaponDict`; -1 = none/unknown
  weapon: z.array(z.number().int().min(-1)),
  // bitfield per frame: 1=alive, 2=hasBomb, 4=hasDefuseKit, 8=flashed
  flags: z.array(nonNegInt.max(15)),
}).strict();
export type ReplayPlayerTrack = z.infer<typeof replayPlayerTrackSchema>;

export const replayRoundSchema = z.object({
  roundNumber: positiveInt,
  startTick: positiveInt,
  tickStep: positiveInt,
  frameCount: nonNegInt,
  players: z.array(replayPlayerTrackSchema),
}).strict();
export type ReplayRound = z.infer<typeof replayRoundSchema>;

export const replaySchema = z.object({
  meta: z.object({
    // frames per second of game time captured (e.g. 8)
    sampleRate: positiveInt,
    tickrate: positiveInt,
    // game units per stored coordinate unit (1 = raw value rounded to int)
    coordScale: positiveInt,
  }).strict(),
  weaponDict: z.array(z.string()),
  rounds: z.array(replayRoundSchema),
}).strict();
export type Replay = z.infer<typeof replaySchema>;

// ── All schemas by manifest file key ─────────────────────────────────────────

export const SCHEMAS_BY_KEY = {
  match: matchSchema,
  players: playersSchema,
  rounds: roundsSchema,
  playerStats: playerStatsSchema,
  playerEconomies: playerEconomiesSchema,
  kills: killsSchema,
  damages: damagesSchema,
  blinds: blindsSchema,
  bombs: bombsSchema,
  clutches: clutchesSchema,
  grenades: grenadesSchema,
  shots: shotsSchema,
  positions1s: positionsSchema,
  replay: replaySchema,
} as const;

/** @deprecated Use SCHEMAS_BY_KEY instead. */
export { SCHEMAS_BY_KEY as FILE_SCHEMAS };
