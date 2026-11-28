/**
 * Check that release version strings across package.json, crates/cs2df/Cargo.toml,
 * and schemas/index.ts match.
 *
 * Run: pnpm check:versions
 */

import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();

const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf-8')) as { version?: string };
const pkgVersion = pkg.version;
if (!pkgVersion) {
  throw new Error('package.json: missing version');
}

const cargoToml = readFileSync(join(root, 'crates', 'cs2df', 'Cargo.toml'), 'utf-8');
const cargoMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
const cargoVersion = cargoMatch?.[1];
if (!cargoVersion) {
  throw new Error('crates/cs2df/Cargo.toml: missing version');
}

const schemasTs = readFileSync(join(root, 'schemas', 'index.ts'), 'utf-8');
const schemasMatch = schemasTs.match(/Canonical Zod Schemas \(v([^)]+)\)/);
const schemasVersion = schemasMatch?.[1];
if (!schemasVersion) {
  throw new Error('schemas/index.ts: missing header version (vX.Y.Z)');
}

const versions: Record<string, string> = {
  'package.json': pkgVersion,
  'crates/cs2df/Cargo.toml': cargoVersion,
  'schemas/index.ts': schemasVersion,
};

const mismatches = Object.entries(versions).filter(([, v]) => v !== pkgVersion);
if (mismatches.length > 0) {
  console.error('Version mismatch:');
  for (const [name, version] of Object.entries(versions)) {
    const marker = version === pkgVersion ? '==' : '!=';
    console.error(`  ${name}: ${version} ${marker} ${pkgVersion}`);
  }
  process.exit(1);
}

console.log(`All release versions match: ${pkgVersion}`);
