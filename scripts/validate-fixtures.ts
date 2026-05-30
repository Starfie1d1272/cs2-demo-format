/**
 * Validate fixture directories against the strict schemas and package-level QA.
 *
 * Run: pnpm validate:fixtures
 */

import { readFileSync, readdirSync, statSync, existsSync } from "fs";
import { join } from "path";
import { SCHEMAS_BY_KEY, manifestSchema } from "../schemas/index.js";

const fixturesDir = join(process.cwd(), "fixtures");
const REQUIRED_KEYS = new Set([
  "match", "players", "rounds", "playerStats", "playerEconomies",
  "kills", "damages", "blinds", "bombs", "grenades", "clutches",
]);
const OPTIONAL_KEYS = new Set(["shots", "positions1s"]);
const EPS = 0.02;

type Row = Record<string, unknown>;

const fixtureDirs = readdirSync(fixturesDir).filter((f: string) =>
  statSync(join(fixturesDir, f)).isDirectory(),
);

let totalFiles = 0;
let errors = 0;
let skippedLegacy = 0;

for (const demo of fixtureDirs) {
  const demoDir = join(fixturesDir, demo);
  console.log(`\nValidating fixture: ${demo}`);

  const dataByKey: Record<string, unknown> = {};
  const manifestPath = join(demoDir, "manifest.json");

  try {
    const manifestRaw = JSON.parse(readFileSync(manifestPath, "utf-8"));
    if (manifestRaw?.schemaVersion !== "cs2-demo-format/2.0") {
      console.log(`  - legacy fixture skipped (schemaVersion: ${String(manifestRaw?.schemaVersion ?? "missing")})`);
      skippedLegacy++;
      continue;
    }

    const manifestResult = manifestSchema.safeParse(manifestRaw);
    dataByKey.manifest = manifestRaw;
    totalFiles++;
    if (!manifestResult.success) {
      reportZodError("manifest.json", manifestResult.error.issues);
      errors += manifestResult.error.issues.length;
    } else {
      console.log(`  ✓ manifest.json (map: ${manifestResult.data.mapName})`);
    }

    const filesMap = isRecord(manifestRaw.files) ? manifestRaw.files : {};
    for (const key of REQUIRED_KEYS) {
      if (!(key in filesMap)) {
        console.error(`  ✗ manifest.files: missing required key "${key}"`);
        errors++;
      }
    }
    for (const key of Object.keys(filesMap)) {
      if (!REQUIRED_KEYS.has(key) && !OPTIONAL_KEYS.has(key)) {
        console.error(`  ✗ manifest.files: unknown key "${key}"`);
        errors++;
      }
    }

    for (const [key, filenameValue] of Object.entries(filesMap)) {
      const filename = String(filenameValue);
      const filePath = join(demoDir, filename);
      const schema = (SCHEMAS_BY_KEY as Record<string, { safeParse: (v: unknown) => { success: boolean; data?: unknown; error?: { issues: Array<{ path: Array<string | number>; message: string }> } } }>)[key];

      if (!existsSync(filePath)) {
        const level = REQUIRED_KEYS.has(key) ? "✗" : "-";
        console.log(`  ${level} ${filename}: declared but not present`);
        if (REQUIRED_KEYS.has(key)) errors++;
        continue;
      }

      const rawText = readFileSync(filePath, "utf-8");
      if (/\b(NaN|-?Infinity)\b/.test(rawText)) {
        console.error(`  ✗ ${filename}: contains invalid JSON numeric values (NaN/Infinity)`);
        errors++;
        continue;
      }

      const raw = JSON.parse(rawText);
      dataByKey[key] = raw;
      if (!schema) {
        console.error(`  ✗ ${filename}: no schema for key "${key}"`);
        errors++;
        continue;
      }

      const result = schema.safeParse(raw);
      const count = Array.isArray(raw) ? raw.length : 1;
      totalFiles++;
      if (result.success) {
        console.log(`  ✓ ${filename} (${count} ${count === 1 ? "row" : "rows"})`);
      } else {
        reportZodError(filename, result.error?.issues ?? []);
        errors += result.error?.issues.length ?? 1;
      }
    }

    errors += runPackageQa(dataByKey);
  } catch (e) {
    console.error(`  ✗ ${demo}: ${(e as Error).message}`);
    errors++;
  }
}

console.log(`\n${errors === 0 ? "✅" : "❌"} ${totalFiles} files checked, ${errors} error(s), ${skippedLegacy} legacy fixture(s) skipped`);
if (errors > 0) process.exit(1);

function reportZodError(filename: string, issues: Array<{ path: Array<string | number>; message: string }>) {
  for (const issue of issues.slice(0, 20)) {
    const path = issue.path.length ? issue.path.join(" → ") : "(root)";
    console.error(`  ✗ ${filename}: [${path}] ${issue.message}`);
  }
  if (issues.length > 20) {
    console.error(`  ✗ ${filename}: ${issues.length - 20} additional schema error(s)`);
  }
}

function runPackageQa(data: Record<string, unknown>): number {
  let qaErrors = 0;
  const players = asRows(data.players);
  const rounds = asRows(data.rounds);
  const stats = asRows(data.playerStats);
  const economies = asRows(data.playerEconomies);
  const kills = asRows(data.kills);
  const damages = asRows(data.damages);
  const blinds = asRows(data.blinds);
  const bombs = asRows(data.bombs);
  const grenades = asRows(data.grenades);
  const clutches = asRows(data.clutches);

  console.log(`  • QA rows: players=${players.length}, rounds=${rounds.length}, kills=${kills.length}, damages=${damages.length}`);

  const playerIds = new Set(players.map((p) => p.steamId64).filter((v): v is string => typeof v === "string"));
  const teamByPlayer = new Map(players.map((p) => [p.steamId64, p.teamKey]));
  const roundNumbers = rounds.map((r) => r.roundNumber).filter((v): v is number => typeof v === "number");
  const roundSet = new Set(roundNumbers);

  const expectedRounds = Array.from({ length: Math.max(0, ...roundNumbers) }, (_, i) => i + 1);
  if (roundNumbers.length && JSON.stringify([...roundNumbers].sort((a, b) => a - b)) !== JSON.stringify(expectedRounds)) {
    qaErrors += qaError("rounds.json: roundNumber must be continuous from 1");
  }

  const badTickRounds: Array<unknown> = [];
  for (const round of rounds) {
    if (round.teamASide === round.teamBSide) {
      qaErrors += qaError(`rounds.json round ${round.roundNumber}: teamASide and teamBSide must differ`);
    }
    if (!(Number(round.startTick) < Number(round.freezeEndTick) && Number(round.freezeEndTick) <= Number(round.endTick))) {
      badTickRounds.push(round.roundNumber);
    }
  }
  if (badTickRounds.length > 0) {
    qaErrors += qaError(`rounds.json: ${badTickRounds.length} row(s) violate tick order start < freezeEnd <= end; sample rounds: ${badTickRounds.slice(0, 8).join(", ")}`);
  }

  for (const [name, rows] of Object.entries({ kills, damages, blinds, bombs, grenades, clutches, playerEconomies: economies })) {
    const missingRounds = new Map<unknown, number>();
    for (const row of rows) {
      if (!roundSet.has(Number(row.roundNumber))) {
        missingRounds.set(row.roundNumber, (missingRounds.get(row.roundNumber) ?? 0) + 1);
      }
    }
    if (missingRounds.size > 0) {
      const sample = [...missingRounds.entries()].slice(0, 8).map(([round, count]) => `${String(round)} (${count})`).join(", ");
      const total = [...missingRounds.values()].reduce((sum, count) => sum + count, 0);
      qaErrors += qaError(`${name}.json: ${total} row(s) reference roundNumber not present in rounds.json; sample: ${sample}`);
    }
  }

  const expectedEconomies = rounds.length * players.length;
  const economyKeys = new Set(economies.map((r) => `${String(r.roundNumber)}:${String(r.steamId64)}`));
  if (economyKeys.size !== expectedEconomies) {
    qaErrors += qaError(`player-economies.json: expected ${expectedEconomies} round/player rows, got ${economyKeys.size}`);
  }

  for (const [name, rows, fields] of [
    ["kills", kills, ["killerSteamId64", "victimSteamId64", "assisterSteamId64", "flashAssisterSteamId64"]],
    ["damages", damages, ["attackerSteamId64", "victimSteamId64"]],
    ["blinds", blinds, ["flasherSteamId64", "flashedSteamId64"]],
    ["bombs", bombs, ["actorSteamId64"]],
    ["grenades", grenades, ["throwerSteamId64"]],
    ["clutches", clutches, ["clutcherSteamId64"]],
    ["playerStats", stats, ["steamId64"]],
  ] as Array<[string, Row[], string[]]>) {
    for (const row of rows) {
      for (const field of fields) {
        const value = row[field];
        if (value !== null && value !== undefined && !playerIds.has(String(value))) {
          qaErrors += qaError(`${name}.json: ${field} ${String(value)} is not present in players.json`);
        }
      }
    }
  }

  for (const row of [...kills, ...damages]) {
    for (const [idField, teamField] of [["killerSteamId64", "killerTeamKey"], ["victimSteamId64", "victimTeamKey"], ["attackerSteamId64", "attackerTeamKey"]]) {
      const id = row[idField];
      const team = row[teamField];
      if (typeof id === "string" && typeof team === "string" && teamByPlayer.get(id) !== team) {
        qaErrors += qaError(`${teamField} does not match players.teamKey for ${id}`);
      }
    }
  }

  const damageByPlayer = new Map<string, number>();
  const utilityByPlayer = new Map<string, number>();
  const utilityWeapons = new Set(["hegrenade", "inferno", "molotov", "incendiary"]);
  for (const row of damages) {
    const raw = Number(row.healthDamageRaw);
    const effective = Number(row.healthDamage);
    const before = Number(row.victimHealthBefore);
    if (Number.isFinite(raw) && Number.isFinite(effective) && Number.isFinite(before) && effective !== Math.min(raw, before)) {
      qaErrors += qaError(`damages.json round ${String(row.roundNumber)} tick ${String(row.tick)}: healthDamage must equal min(healthDamageRaw, victimHealthBefore)`);
    }
    if (typeof row.attackerSteamId64 === "string" && row.attackerTeamKey !== row.victimTeamKey && Number.isFinite(effective)) {
      damageByPlayer.set(row.attackerSteamId64, (damageByPlayer.get(row.attackerSteamId64) ?? 0) + effective);
      if (utilityWeapons.has(String(row.weapon))) {
        utilityByPlayer.set(row.attackerSteamId64, (utilityByPlayer.get(row.attackerSteamId64) ?? 0) + effective);
      }
    }
  }

  for (const row of stats) {
    const sid = String(row.steamId64);
    if (row.rounds !== rounds.length) {
      qaErrors += qaError(`player-stats.json ${sid}: rounds must equal rounds.length (${rounds.length})`);
    }
    qaErrors += expectEqual(`player-stats.json ${sid}: damageHealth`, row.damageHealth, damageByPlayer.get(sid) ?? 0);
    qaErrors += expectEqual(`player-stats.json ${sid}: utilityDamage`, row.utilityDamage, utilityByPlayer.get(sid) ?? 0);
    if (typeof row.rounds === "number" && row.rounds > 0) {
      qaErrors += expectClose(`player-stats.json ${sid}: adr`, row.adr, Number(row.damageHealth) / row.rounds);
      qaErrors += expectClose(`player-stats.json ${sid}: averageUtilityDamagePerRound`, row.averageUtilityDamagePerRound, Number(row.utilityDamage) / row.rounds);
      qaErrors += expectClose(`player-stats.json ${sid}: kast`, row.kast, Number(row.kast_rounds) / row.rounds * 100);
    }
  }

  for (const row of kills) {
    if (row.flashAssist === true && !row.flashAssisterSteamId64) {
      qaErrors += qaError(`kills.json round ${String(row.roundNumber)} tick ${String(row.tick)}: flashAssist=true requires flashAssisterSteamId64`);
    }
  }

  return qaErrors;
}

function asRows(value: unknown): Row[] {
  return Array.isArray(value) ? value.filter(isRecord) : [];
}

function isRecord(value: unknown): value is Row {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function qaError(message: string): number {
  console.error(`  ✗ QA: ${message}`);
  return 1;
}

function expectEqual(label: string, actual: unknown, expected: number): number {
  return actual === expected ? 0 : qaError(`${label} expected ${expected}, got ${String(actual)}`);
}

function expectClose(label: string, actual: unknown, expected: number): number {
  return typeof actual === "number" && Math.abs(actual - expected) <= EPS
    ? 0
    : qaError(`${label} expected ${expected.toFixed(3)}, got ${String(actual)}`);
}
