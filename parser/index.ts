/**
 * cs2-demo-format — Reference Parser (v3.x forward-compatible)
 *
 * Parses a CS2 demo export ZIP buffer into a typed in-memory object.
 * Validates every known file against the schemas defined in `schemas/index.ts`,
 * while tolerating future 3.x minor versions, unknown optional fields, and
 * unregistered optional manifest file keys.
 *
 * Usage:
 *   import { parseDemoPackage } from "cs2-demo-format/parser";
 *   const parsed = await parseDemoPackage(fs.readFileSync("export.zip"));
 *   console.log(parsed.files.playerStats); // typed PlayerStatsRow[]
 *
 * v3 streams (shots/replay/duels) are columnar with delta-encoded arrays; use
 * `decodeDelta` (re-exported here) or `decodeTrackFrames` to materialize
 * absolute per-frame values.
 */

import JSZip from 'jszip';
import { z } from 'zod';
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
  type WeaponEventRow,
} from '../schemas/index.js';

export {
  decodeDelta,
  FLAG_ALIVE,
  FLAG_HAS_BOMB,
  FLAG_HAS_DEFUSE_KIT,
  FLAG_HAS_HELMET,
  FLAG_IS_SCOPED,
  FLAG_IS_WALKING,
  FLAG_IS_AIRBORNE,
  FLAG_IS_DUCKING,
  FLAG_IS_IN_RELOAD,
} from '../schemas/index.js';

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
    weapons?: WeaponEventRow[];
  };
}

/**
 * Recursively strip `.strict()` from a Zod schema so consumers tolerate
 * additional optional fields introduced in future `3.x` minor versions.
 */
function loosenSchema<T extends z.ZodTypeAny>(schema: T): z.ZodTypeAny {
  const def = (
    schema as unknown as {
      def?: {
        type?: string;
        element?: z.ZodTypeAny;
        innerType?: z.ZodTypeAny;
        defaultValue?: unknown;
      };
    }
  ).def;
  const type = def?.type;
  if (type === 'object') {
    const obj = schema as unknown as { shape: Record<string, z.ZodTypeAny> };
    const nextShape: Record<string, z.ZodTypeAny> = {};
    for (const [k, v] of Object.entries(obj.shape)) {
      nextShape[k] = loosenSchema(v);
    }
    return z.object(nextShape);
  }
  if (type === 'array' && def?.element) {
    return z.array(loosenSchema(def.element));
  }
  if (type === 'optional' && def?.innerType) {
    return loosenSchema(def.innerType).optional();
  }
  if (type === 'nullable' && def?.innerType) {
    return loosenSchema(def.innerType).nullable();
  }
  if (type === 'default' && def?.innerType) {
    return loosenSchema(def.innerType).default(def.defaultValue);
  }
  return schema;
}

const consumerManifestSchema = (
  loosenSchema(manifestSchema) as z.ZodObject<Record<string, z.ZodTypeAny>>
).extend({
  schemaVersion: z.string().regex(/^cs2-demo-format\/3\.\d+$/),
  files: (
    loosenSchema(manifestSchema.shape.files) as z.ZodObject<Record<string, z.ZodTypeAny>>
  ).catchall(z.string().min(1)),
});

const CONSUMER_SCHEMAS_BY_KEY: Record<string, z.ZodTypeAny> = Object.fromEntries(
  Object.entries(SCHEMAS_BY_KEY).map(([k, s]) => [k, loosenSchema(s)]),
);

/**
 * Parse a CS2 demo export ZIP into validated, typed in-memory data.
 *
 * Forward-compatible across `cs2-demo-format/3.x`: unknown optional fields
 * and unregistered optional manifest file keys are tolerated without error.
 *
 * @param buffer  ZIP file contents as Buffer or ArrayBuffer.
 * @throws        If the ZIP is malformed, missing required files, or fails schema validation.
 */
export async function parseDemoPackage(buffer: Buffer | ArrayBuffer): Promise<ParsedDemoPackage> {
  const zip = await JSZip.loadAsync(buffer);

  const manifestFile = zip.file('manifest.json');
  if (!manifestFile) throw new Error('ZIP is missing manifest.json');

  const manifestRaw = JSON.parse(await manifestFile.async('text'));
  const manifest = consumerManifestSchema.parse(manifestRaw) as Manifest;

  const files: Record<string, unknown> = {};

  for (const [key, filename] of Object.entries(manifest.files)) {
    const entry = zip.file(filename);
    if (!entry) throw new Error(`ZIP is missing file: ${filename} (key: ${key})`);

    const schema = CONSUMER_SCHEMAS_BY_KEY[key];
    if (!schema) {
      // Forward-compatible: skip unregistered optional file keys from future 3.x minor versions.
      continue;
    }

    const raw = JSON.parse(await entry.async('text'));
    files[key] = schema.parse(raw);
  }

  return { manifest, files: files as unknown as ParsedDemoPackage['files'] };
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
