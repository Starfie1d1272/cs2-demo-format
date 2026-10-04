/**
 * Generate language-neutral JSON Schema files from the canonical Zod schemas.
 *
 * Output: spec/<key>.schema.json — one file per manifest file key.
 * These files are committed to the repo and consumed by non-TypeScript tooling
 * (Python jsonschema/pydantic, etc.) without any Node.js dependency.
 *
 * Run: pnpm gen:schema
 */

import { z } from "zod";
import { writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { SCHEMAS_BY_KEY, manifestSchema } from "../schemas/index.js";

const specDir = join(process.cwd(), "spec");

mkdirSync(specDir, { recursive: true });

const entries: Array<[string, z.ZodType]> = [
  ["manifest", manifestSchema],
  ...Object.entries(SCHEMAS_BY_KEY),
];

for (const [key, schema] of entries) {
  const jsonSchema = z.toJSONSchema(schema, {
    target: "draft-7",
  });

  const outPath = join(specDir, `${key}.schema.json`);
  writeFileSync(outPath, JSON.stringify(jsonSchema, null, 2) + "\n");
  console.log(`  wrote ./spec/${key}.schema.json`);
}

console.log(`\nGenerated ${entries.length} JSON Schema files in spec/`);
