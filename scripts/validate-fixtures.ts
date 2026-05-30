/**
 * Validate all fixtures against the canonical schemas.
 *
 * For each fixture directory, reads every JSON file and validates it
 * against the matching schema from SCHEMAS_BY_KEY.
 *
 * Run: pnpm validate:fixtures
 */

import { readFileSync, readdirSync, statSync, existsSync } from "fs";
import { join } from "path";
import { SCHEMAS_BY_KEY, manifestSchema } from "../schemas/index.js";

/** Mirror of parser sanitization: NaN / ±Infinity → null before JSON.parse */
function sanitize(text: string): string {
  return text.replace(/\bNaN\b/g, "null").replace(/\b-?Infinity\b/g, "null");
}

const fixturesDir = join(process.cwd(), "fixtures");

const fixtureDirs = readdirSync(fixturesDir).filter((f: string) =>
  statSync(join(fixturesDir, f)).isDirectory(),
);

if (fixtureDirs.length === 0) {
  console.log("No fixture directories found — skipping.");
  process.exit(0);
}

let totalFiles = 0;
let errors = 0;

for (const demo of fixtureDirs) {
  const demoDir = join(fixturesDir, demo);
  console.log(`\nValidating fixture: ${demo}`);

  // Parse manifest first to get the file key → filename mapping
  const manifestPath = join(demoDir, "manifest.json");
  const manifest = manifestSchema.parse(JSON.parse(readFileSync(manifestPath, "utf-8")));
  console.log(`  manifest OK (map: ${manifest.mapName})`);
  totalFiles++;

  for (const [key, filename] of Object.entries(manifest.files)) {
    const filePath = join(demoDir, filename);
    const schema = (SCHEMAS_BY_KEY as Record<string, { safeParse: (v: unknown) => { success: boolean; error?: { message: string } } }>)[key];

    if (!schema) {
      console.log(`  [SKIP] ${filename} — no schema for key "${key}"`);
      continue;
    }

    try {
      if (!existsSync(filePath)) {
        console.log(`  [SKIP] ${filename} — not present in fixture (optional)`);
        continue;
      }
      const raw = JSON.parse(sanitize(readFileSync(filePath, "utf-8")));
      const result = schema.safeParse(raw);
      if (result.success) {
        const count = Array.isArray(raw) ? raw.length : 1;
        console.log(`  ✓ ${filename} (${count} rows)`);
        totalFiles++;
      } else {
        console.error(`  ✗ ${filename}: ${result.error?.message}`);
        errors++;
      }
    } catch (e) {
      console.error(`  ✗ ${filename}: ${(e as Error).message}`);
      errors++;
    }
  }
}

console.log(`\n${errors === 0 ? "✅" : "❌"} ${totalFiles} files validated, ${errors} errors`);
if (errors > 0) process.exit(1);
