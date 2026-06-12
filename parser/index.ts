/**
 * cs2-demo-format — Reference Parser (v3)
 *
 * Parses a CS2 demo export ZIP buffer into a typed in-memory object.
 * Validates every file against the canonical schemas defined in `schemas/index.ts`.
 * The parser is intentionally strict: producers must emit valid JSON and must
 * not rely on consumers to sanitize NaN/Infinity or filter warmup rows.
 *
 * Usage:
 *   import { parseDemoPackage } from "cs2-demo-format/parser";
 *   const parsed = await parseDemoPackage(fs.readFileSync("export.zip"));
 *   console.log(parsed.files.playerStats); // typed PlayerStatsRow[]
 *
 * v3 streams (shots/replay/duels) are columnar with delta-encoded arrays; use
 * `decodeDelta` (re-exported here) or `decodeReplayTrack` to materialize
 * absolute per-frame values.
 */

import JSZip from "jszip";
import {
  manifestSchema,
  SCHEMAS_BY_KEY,
  decodeDelta,
  type Manifest,
  type Match,
  type PlayerRow,
  type RoundRow,
  type PlayerStatsRow,
  type PlayerEconomyRow,
  type KillRow,
  type DamageRow,
  type BlindRow,
  type BombRow,
  type ClutchRow,
  type GrenadeRow,
  type Shots,
  type Replay,
  type ReplayPlayerTrack,
  type Duels,
  type DuelPlayerTrack,
} from "../schemas/index.js";

export { decodeDelta, FLAG_ALIVE, FLAG_HAS_BOMB, FLAG_HAS_DEFUSE_KIT } from "../schemas/index.js";

export interface ParsedDemoPackage {
  manifest: Manifest;
  files: {
    match: Match;
    players: PlayerRow[];
    rounds: RoundRow[];
    playerStats: PlayerStatsRow[];
    playerEconomies: PlayerEconomyRow[];
    kills: KillRow[];
    damages: DamageRow[];
    blinds: BlindRow[];
    bombs: BombRow[];
    clutches: ClutchRow[];
    grenades: GrenadeRow[];
    shots?: Shots;
    replay?: Replay;
    duels?: Duels;
  };
}

/**
 * Parse a CS2 demo export ZIP into validated, typed in-memory data.
 *
 * @param buffer  ZIP file contents as Buffer or ArrayBuffer.
 * @throws        If the ZIP is malformed, missing required files, or fails schema validation.
 */
export async function parseDemoPackage(buffer: Buffer | ArrayBuffer): Promise<ParsedDemoPackage> {
  const zip = await JSZip.loadAsync(buffer);

  const manifestFile = zip.file("manifest.json");
  if (!manifestFile) throw new Error("ZIP is missing manifest.json");

  const manifestRaw = JSON.parse(await manifestFile.async("text"));
  const manifest = manifestSchema.parse(manifestRaw);

  const files: Record<string, unknown> = {};

  for (const [key, filename] of Object.entries(manifest.files)) {
    const entry = zip.file(filename);
    if (!entry) throw new Error(`ZIP is missing file: ${filename} (key: ${key})`);

    const raw = JSON.parse(await entry.async("text"));
    const schema = (SCHEMAS_BY_KEY as Record<string, { parse: (v: unknown) => unknown }>)[key];

    if (!schema) throw new Error(`Unknown manifest file key: ${key}`);

    files[key] = schema.parse(raw);
  }

  return { manifest, files: files as unknown as ParsedDemoPackage["files"] };
}

/** Absolute per-frame values decoded from a replay player track. */
export interface DecodedTrackFrame {
  tick: number;
  /** game units (coordScale applied) */
  x: number;
  y: number;
  z: number;
  /** degrees (angleScale applied) */
  yaw: number;
  pitch: number;
  hp: number;
  flash: number;
}

/**
 * Decode a replay or duels player track into absolute per-frame values.
 * Applies delta decoding plus coordScale/angleScale from the stream meta.
 */
export function decodeTrackFrames(
  track: ReplayPlayerTrack | DuelPlayerTrack,
  round: { startTick: number; tickStep: number; frameCount: number },
  meta: { coordScale: number; angleScale: number },
): DecodedTrackFrame[] {
  const x = decodeDelta(track.x);
  const y = decodeDelta(track.y);
  const z = decodeDelta(track.z);
  const yaw = decodeDelta(track.yaw);
  const pitch = decodeDelta(track.pitch);
  const out: DecodedTrackFrame[] = [];
  for (let i = 0; i < round.frameCount; i++) {
    out.push({
      tick: round.startTick + i * round.tickStep,
      x: x[i] * meta.coordScale,
      y: y[i] * meta.coordScale,
      z: z[i] * meta.coordScale,
      yaw: yaw[i] / meta.angleScale,
      pitch: pitch[i] / meta.angleScale,
      hp: track.hp[i],
      flash: track.flash[i] / 10,
    });
  }
  return out;
}
