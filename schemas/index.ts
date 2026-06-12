/**
 * cs2-demo-format — Canonical Zod Schemas (v3.0.0)
 *
 * Strict export contract for CS2 demo ZIP packages.
 * `schemas/index.ts` is the single source of truth; JSON Schema files in
 * `spec/` are generated from these definitions.
 *
 * Schema version string: "cs2-demo-format/3.0".
 *
 * ── v3 core conventions ──────────────────────────────────────────────────────
 *
 * Player references: every file except `players.json` refers to players by
 * `playerIndex` — the row index into the `players.json` array. The 17-digit
 * steamId64 appears exactly once per player, in players.json.
 *
 * Team/side derivation: per-row `teamKey` / `side` fields were removed in v3.
 * Consumers derive them: `players[playerIndex].teamKey` gives the team, and
 * `rounds[roundNumber].teamASide / teamBSide` gives the side for that round.
 *
 * Integer-only payloads: positions are integer game units, angles are integers
 * in `degrees × angleScale` (meta-declared, default 10 = 0.1°), durations in
 * event files are seconds (float allowed there only), flash columns in streams
 * are tenths of a second. High-frequency streams contain no floats at all.
 *
 * Delta encoding: array fields documented as "delta" store the first element
 * as an absolute value and every subsequent element as the difference from the
 * previous frame: stored[0] = v[0], stored[i] = v[i] − v[i−1]. Decode with a
 * running prefix sum. Non-delta arrays store plain per-frame values.
 */

import { z } from "zod";

// ── Shared primitives ──────────────────────────────────────────────────────────

export const steamId64Schema = z.string().regex(/^\d{17}$/);
export const teamKeySchema = z.enum(["teamA", "teamB"]);
export const sideSchema = z.enum(["t", "ct"]);
export const economyTypeSchema = z.enum(["pistol", "eco", "semi", "force", "full"]);
/**
 * Team-level economy summary per round (majority vote of the five individual
 * player economy types). Pistol-conversion rounds (R2 / R14 where the team
 * won the previous pistol round) are classified as `"full"` — the winners
 * have enough money for rifles and equipment, and the losers are on eco or
 * force; the "won pistol" context is implicit from `roundNumber` and the
 * `winnerTeamKey` of the previous round.
 */
export const teamEconomyTypeSchema = z.enum(["pistol", "eco", "semi", "force", "full"]);
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

/** Integer position in game units (sub-unit precision is parser noise). */
export const vec3Schema = z.object({
  x: z.number().int(),
  y: z.number().int(),
  z: z.number().int(),
}).strict();

const nonNegInt = z.number().int().min(0);
const positiveInt = z.number().int().min(1);
const nonNegNumber = z.number().min(0);
const percentage = z.number().min(0).max(100);
const weaponString = z.string().min(1);
const nullableString = z.string().nullable();

/** Index into the players.json array. */
export const playerIndexSchema = nonNegInt;
const nullablePlayerIndex = playerIndexSchema.nullable();
const nullableVec3 = vec3Schema.nullable();

/** Plain integer array (per-frame values, no delta). */
const intArray = z.array(z.number().int());
/** Delta-encoded integer array: [0] absolute, [i] = v[i] − v[i−1]. */
const deltaIntArray = z.array(z.number().int());
/** Dictionary-index array; -1 = none/unknown. */
const dictIndexArray = z.array(z.number().int().min(-1));

// ── manifest.json ─────────────────────────────────────────────────────────────

export const manifestSchema = z.object({
  schemaVersion: z.literal("cs2-demo-format/3.0"),
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
    replay: z.string().min(1).optional(),
    // Full-tick combat-window stream for reaction-time research (optional;
    // produced by research-profile exports).
    duels: z.string().min(1).optional(),
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
//
// Row order is normative: `playerIndex` fields across the whole package are
// indexes into this array. Producers must keep the array stable within a
// package (sorted by teamKey then steamId64 is recommended but not required).

export const playerRowSchema = z.object({
  steamId64: steamId64Schema,
  name: z.string().min(1),
  teamKey: teamKeySchema,
}).strict();
export const playersSchema = z.array(playerRowSchema);
export type PlayerRow = z.infer<typeof playerRowSchema>;

// ── rounds.json ───────────────────────────────────────────────────────────────
//
// Kept fully denormalized: this file is the derivation source for per-round
// side lookups (playerIndex → teamKey → teamASide/teamBSide).

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
  playerIndex: playerIndexSchema,
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
  kastRounds: nonNegInt,
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
  playerIndex: playerIndexSchema,
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
  killerIndex: nullablePlayerIndex,
  victimIndex: playerIndexSchema,
  assisterIndex: nullablePlayerIndex,
  flashAssisterIndex: nullablePlayerIndex,
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
  attackerIndex: nullablePlayerIndex,
  victimIndex: playerIndexSchema,
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
  // victimHealthAfter = victimHealthBefore − healthDamage (removed in v3);
  // victimArmorBefore = victimArmorAfter + armorDamage (removed in v3).
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
  flasherIndex: playerIndexSchema,
  flashedIndex: playerIndexSchema,
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
  actorIndex: nullablePlayerIndex,
  position: vec3Schema,
}).strict();
export const bombsSchema = z.array(bombRowSchema);
export type BombRow = z.infer<typeof bombRowSchema>;

// ── clutches.json ─────────────────────────────────────────────────────────────

export const clutchRowSchema = z.object({
  roundNumber: positiveInt,
  tick: positiveInt,
  clutcherIndex: playerIndexSchema,
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
  throwerIndex: playerIndexSchema,
  throwPosition: vec3Schema,
  effectPosition: vec3Schema,
}).strict();
export const grenadesSchema = z.array(grenadeRowSchema);
export type GrenadeRow = z.infer<typeof grenadeRowSchema>;

// ── shots.json (columnar weapon-fire stream) ──────────────────────────────────
//
// One track per (roundNumber, playerIndex); parallel arrays share one length.
// `tick`, `x`, `y`, `z`, `yaw`, `pitch` are delta-encoded; velocity is plain
// per-shot (frame-to-frame velocity is uncorrelated, deltas would not help).
// Angles are integers in degrees × meta.angleScale.

export const shotTrackSchema = z.object({
  roundNumber: positiveInt,
  playerIndex: playerIndexSchema,
  /** delta; absolute ticks of each shot */
  tick: deltaIntArray,
  /** index into weaponDict */
  weapon: dictIndexArray,
  /** delta; shooter position in game units */
  x: deltaIntArray,
  y: deltaIntArray,
  z: deltaIntArray,
  /** per-shot velocity in game units/s (plain) */
  vx: intArray,
  vy: intArray,
  vz: intArray,
  /** delta; view angles in degrees × angleScale */
  yaw: deltaIntArray,
  pitch: deltaIntArray,
}).strict();
export type ShotTrack = z.infer<typeof shotTrackSchema>;

export const shotsSchema = z.object({
  meta: z.object({
    /** game units per stored coordinate unit (1 = raw rounded) */
    coordScale: positiveInt,
    /** stored angle = degrees × angleScale (10 = 0.1° resolution) */
    angleScale: positiveInt,
  }).strict(),
  weaponDict: z.array(z.string()),
  tracks: z.array(shotTrackSchema),
}).strict();
export type Shots = z.infer<typeof shotsSchema>;

// ── replay.json (unified columnar player-state stream) ───────────────────────
//
// THE positional/state stream of the package (v3 merged the former
// positions-1s.json into this file). Default rate is 8 Hz; consumers needing
// ~1 Hz analytics (heatmaps, economy curves) stride by meta.sampleRate.
//
// Per round, each player carries parallel arrays of length `frameCount`. The
// tick of frame `i` is `startTick + i * tickStep`. While a player is dead or
// disconnected (flags alive bit = 0) the per-frame values are unspecified;
// producers SHOULD repeat the last live value so delta streams stay compact.

export const replayPlayerTrackSchema = z.object({
  playerIndex: playerIndexSchema,
  /** delta; position in game units / meta.coordScale */
  x: deltaIntArray,
  y: deltaIntArray,
  z: deltaIntArray,
  /** delta; view angles in degrees × meta.angleScale */
  yaw: deltaIntArray,
  pitch: deltaIntArray,
  /** plain; health 0–100 */
  hp: intArray,
  /** plain; armor 0–100 */
  armor: intArray,
  /** delta; cash balance */
  money: deltaIntArray,
  /** delta; current equipment value */
  equipValue: deltaIntArray,
  /** index into weaponDict; -1 = none/unknown */
  weapon: dictIndexArray,
  /** index into placeDict (CS2 callout names); -1 = between callouts */
  place: dictIndexArray,
  /** plain; remaining flash-blind duration in tenths of a second (0–60) */
  flash: intArray,
  /** plain bitfield: 1 = alive, 2 = hasBomb, 4 = hasDefuseKit */
  flags: intArray,
}).strict();
export type ReplayPlayerTrack = z.infer<typeof replayPlayerTrackSchema>;

// A single thrown grenade's in-flight path, on the SAME time grid as player
// tracks. Frame `i` is at `startTick + i * tickStep` (the round's tickStep).
// Covers the flight phase only (throw → detonate); the static effect afterwards
// (smoke cloud / fire area) lives in grenades.json via effectPosition +
// destroyTick. Coordinates follow the same coordScale/delta convention.
export const replayProjectileSchema = z.object({
  grenade: grenadeTypeSchema,
  throwerIndex: nullablePlayerIndex,
  startTick: positiveInt,
  /** delta; projectile position in game units / meta.coordScale */
  x: deltaIntArray,
  y: deltaIntArray,
  z: deltaIntArray,
}).strict();
export type ReplayProjectile = z.infer<typeof replayProjectileSchema>;

export const replayRoundSchema = z.object({
  roundNumber: positiveInt,
  startTick: positiveInt,
  tickStep: positiveInt,
  frameCount: nonNegInt,
  players: z.array(replayPlayerTrackSchema),
  projectiles: z.array(replayProjectileSchema),
}).strict();
export type ReplayRound = z.infer<typeof replayRoundSchema>;

export const replaySchema = z.object({
  meta: z.object({
    /** frames per second of game time captured (e.g. 8) */
    sampleRate: positiveInt,
    tickrate: positiveInt,
    /** game units per stored coordinate unit (1 = raw value rounded to int) */
    coordScale: positiveInt,
    /** stored angle = degrees × angleScale (10 = 0.1° resolution) */
    angleScale: positiveInt,
  }).strict(),
  weaponDict: z.array(z.string()),
  placeDict: z.array(z.string()),
  rounds: z.array(replayRoundSchema),
}).strict();
export type Replay = z.infer<typeof replaySchema>;

// ── duels.json (full-tick combat-window stream, research profile) ────────────
//
// High-frequency sampling around combat for reaction-time and duel analysis.
// Windows are built per round from kill/damage anchor events: each anchor
// spans [tick − windowBeforeMs, tick + windowAfterMs]; overlapping spans in
// the same round are merged into one window. All players alive anywhere in
// the window are included for the whole window. Default sampleRate equals the
// demo tickrate (tickStep = 1). Combine with shots.json (exact fire ticks) and
// the flash column to measure visual-stimulus → first-shot latency.

export const duelAnchorSchema = z.object({
  kind: z.enum(["kill", "damage"]),
  tick: positiveInt,
  attackerIndex: nullablePlayerIndex,
  victimIndex: playerIndexSchema,
}).strict();
export type DuelAnchor = z.infer<typeof duelAnchorSchema>;

export const duelPlayerTrackSchema = z.object({
  playerIndex: playerIndexSchema,
  /** delta; position in game units / meta.coordScale */
  x: deltaIntArray,
  y: deltaIntArray,
  z: deltaIntArray,
  /** delta; view angles in degrees × meta.angleScale */
  yaw: deltaIntArray,
  pitch: deltaIntArray,
  /** plain; health 0–100 (0 = dead) */
  hp: intArray,
  /** plain; remaining flash-blind duration in tenths of a second (0–60) */
  flash: intArray,
}).strict();
export type DuelPlayerTrack = z.infer<typeof duelPlayerTrackSchema>;

export const duelWindowSchema = z.object({
  roundNumber: positiveInt,
  startTick: positiveInt,
  tickStep: positiveInt,
  frameCount: nonNegInt,
  anchors: z.array(duelAnchorSchema).min(1),
  players: z.array(duelPlayerTrackSchema),
}).strict();
export type DuelWindow = z.infer<typeof duelWindowSchema>;

export const duelsSchema = z.object({
  meta: z.object({
    tickrate: positiveInt,
    /** frames per second of game time captured (= tickrate for full-tick) */
    sampleRate: positiveInt,
    coordScale: positiveInt,
    angleScale: positiveInt,
    /** anchor window extent before/after the anchor tick, in milliseconds */
    windowBeforeMs: positiveInt,
    windowAfterMs: positiveInt,
  }).strict(),
  windows: z.array(duelWindowSchema),
}).strict();
export type Duels = z.infer<typeof duelsSchema>;

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
  replay: replaySchema,
  duels: duelsSchema,
} as const;

/** @deprecated Use SCHEMAS_BY_KEY instead. */
export { SCHEMAS_BY_KEY as FILE_SCHEMAS };

// ── Decode helpers ────────────────────────────────────────────────────────────

/** Decode a delta-encoded integer array back to absolute values. */
export function decodeDelta(deltas: readonly number[]): number[] {
  const out = new Array<number>(deltas.length);
  let acc = 0;
  for (let i = 0; i < deltas.length; i++) {
    acc += deltas[i];
    out[i] = acc;
  }
  return out;
}

/** Replay/duels flags bit values. */
export const FLAG_ALIVE = 1;
export const FLAG_HAS_BOMB = 2;
export const FLAG_HAS_DEFUSE_KIT = 4;
