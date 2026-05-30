/**
 * cs2-demo-format — Reference Parser
 *
 * Parses a CS2 demo export ZIP buffer into a typed in-memory object.
 * Validates every file against the canonical schemas defined in `schemas/index.ts`.
 *
 * Usage:
 *   import { parseDemoPackage } from "cs2-demo-format/parser";
 *   const parsed = await parseDemoPackage(fs.readFileSync("export.zip"));
 *   console.log(parsed.files.playerStats); // typed PlayerStatsRow[]
 */

import JSZip from "jszip";
import { manifestSchema, SCHEMAS_BY_KEY, type Manifest } from "../schemas/index.js";

/**
 * These files contain a `roundNumber` field where warmup rows (roundNumber=0)
 * have null/unknown steamId64/side values and are meaningless for analysis.
 * They are filtered out automatically during parsing.
 */
const WARMUP_FILTERED_KEYS = new Set([
  "kills", "damages", "blinds", "bombs",
  "grenades", "shots", "clutches",
  "playerEconomies", "rounds", "positions1s",
]);

export interface ParsedDemoPackage {
  manifest: Manifest;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  files: Record<string, any[]>;
}

/** Sanitize JSON text: replace bare NaN / ±Infinity with null (not valid JSON). */
function sanitizeJsonText(text: string): string {
  return text.replace(/\bNaN\b/g, "null").replace(/\b-?Infinity\b/g, "null");
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

  const manifestRaw = JSON.parse(sanitizeJsonText(await manifestFile.async("text")));
  const manifest = manifestSchema.parse(manifestRaw);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const files: Record<string, any[]> = {};

  for (const [key, filename] of Object.entries(manifest.files)) {
    const entry = zip.file(filename);
    if (!entry) throw new Error(`ZIP is missing file: ${filename} (key: ${key})`);

    const raw = JSON.parse(sanitizeJsonText(await entry.async("text")));
    const schema = (SCHEMAS_BY_KEY as Record<string, { parse: (v: unknown) => unknown }>)[key];

    if (!schema) throw new Error(`Unknown manifest file key: ${key}`);

    const parsed = schema.parse(raw);
    // match.json is a single object — wrap in array for uniform access
    let rows = Array.isArray(parsed) ? parsed : [parsed];

    // Filter warmup rows (roundNumber=0): exporters include warmup events but
    // their steamId64/side fields are null/unknown, making them useless for stats.
    if (WARMUP_FILTERED_KEYS.has(key)) {
      rows = rows.filter((r: { roundNumber?: number }) => (r.roundNumber ?? 1) > 0);
    }

    files[key] = rows;
  }

  return { manifest, files };
}
