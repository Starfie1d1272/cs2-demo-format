/**
 * Validate fixture directories against the strict v3 schemas and package-level QA.
 *
 * Run: pnpm validate:fixtures
 *
 * Fixture layout: fixtures/<name>/ holds the extracted package files
 * (manifest.json + the files it declares). Legacy (< v3) fixtures are skipped.
 */

import { readFileSync, readdirSync, statSync, existsSync } from "fs";
import { join } from "path";
import { SCHEMAS_BY_KEY, manifestSchema, decodeDelta } from "../schemas/index.js";

const fixturesDir = join(process.cwd(), "fixtures");
const REQUIRED_KEYS = new Set([
  "match", "players", "rounds", "playerStats", "playerEconomies",
  "kills", "damages", "blinds", "bombs", "grenades", "clutches",
]);
const OPTIONAL_KEYS = new Set(["shots", "replay", "duels"]);
const SCHEMA_VERSION = "cs2-demo-format/3.0";
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
    if (manifestRaw?.schemaVersion !== SCHEMA_VERSION) {
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
  const replay = isRecord(data.replay) ? data.replay : undefined;
  const duels = isRecord(data.duels) ? data.duels : undefined;
  const shots = isRecord(data.shots) ? data.shots : undefined;

  console.log(`  • QA rows: players=${players.length}, rounds=${rounds.length}, kills=${kills.length}, damages=${damages.length}`);

  const nPlayers = players.length;
  const teamByIndex = players.map((p) => p.teamKey);
  const roundNumbers = rounds.map((r) => r.roundNumber).filter((v): v is number => typeof v === "number");
  const roundSet = new Set(roundNumbers);
  const roundsByNumber = new Map(rounds.map((r) => [r.roundNumber, r]));

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

  const expectedEconomies = rounds.length * nPlayers;
  const economyKeys = new Set(economies.map((r) => `${String(r.roundNumber)}:${String(r.playerIndex)}`));
  if (economyKeys.size !== expectedEconomies) {
    qaErrors += qaError(`player-economies.json: expected ${expectedEconomies} round/player rows, got ${economyKeys.size}`);
  }

  // playerIndex references in range
  const indexOk = (v: unknown): boolean => typeof v === "number" && Number.isInteger(v) && v >= 0 && v < nPlayers;
  for (const [name, rows, fields] of [
    ["kills", kills, ["killerIndex", "victimIndex", "assisterIndex", "flashAssisterIndex"]],
    ["damages", damages, ["attackerIndex", "victimIndex"]],
    ["blinds", blinds, ["flasherIndex", "flashedIndex"]],
    ["bombs", bombs, ["actorIndex"]],
    ["grenades", grenades, ["throwerIndex"]],
    ["clutches", clutches, ["clutcherIndex"]],
    ["playerEconomies", economies, ["playerIndex"]],
    ["playerStats", stats, ["playerIndex"]],
  ] as Array<[string, Row[], string[]]>) {
    for (const row of rows) {
      for (const field of fields) {
        const value = row[field];
        if (value !== null && value !== undefined && !indexOk(value)) {
          qaErrors += qaError(`${name}.json: ${field} ${String(value)} is not a valid players.json index`);
        }
      }
    }
  }

  const damageByPlayer = new Map<number, number>();
  const utilityByPlayer = new Map<number, number>();
  const utilityWeapons = new Set(["hegrenade", "inferno", "molotov", "incendiary"]);
  for (const row of damages) {
    const raw = Number(row.healthDamageRaw);
    const effective = Number(row.healthDamage);
    const before = Number(row.victimHealthBefore);
    if (Number.isFinite(raw) && Number.isFinite(effective) && Number.isFinite(before) && effective !== Math.min(raw, before)) {
      qaErrors += qaError(`damages.json round ${String(row.roundNumber)} tick ${String(row.tick)}: healthDamage must equal min(healthDamageRaw, victimHealthBefore)`);
    }
    const atk = row.attackerIndex;
    const vic = row.victimIndex;
    if (typeof atk === "number" && typeof vic === "number" && atk !== vic
        && teamByIndex[atk] !== teamByIndex[vic] && Number.isFinite(effective)) {
      damageByPlayer.set(atk, (damageByPlayer.get(atk) ?? 0) + effective);
      if (utilityWeapons.has(String(row.weapon))) {
        utilityByPlayer.set(atk, (utilityByPlayer.get(atk) ?? 0) + effective);
      }
    }
  }

  for (const row of stats) {
    const idx = Number(row.playerIndex);
    const label = `player-stats.json playerIndex=${idx}`;
    if (row.rounds !== rounds.length) {
      qaErrors += qaError(`${label}: rounds must equal rounds.length (${rounds.length})`);
    }
    qaErrors += expectEqual(`${label}: damageHealth`, row.damageHealth, damageByPlayer.get(idx) ?? 0);
    qaErrors += expectEqual(`${label}: utilityDamage`, row.utilityDamage, utilityByPlayer.get(idx) ?? 0);
    if (typeof row.rounds === "number" && row.rounds > 0) {
      qaErrors += expectClose(`${label}: adr`, row.adr, Number(row.damageHealth) / row.rounds);
      qaErrors += expectClose(`${label}: averageUtilityDamagePerRound`, row.averageUtilityDamagePerRound, Number(row.utilityDamage) / row.rounds);
      qaErrors += expectClose(`${label}: kast`, row.kast, Number(row.kastRounds) / row.rounds * 100);
    }
  }

  for (const row of kills) {
    if (row.flashAssist === true && (row.flashAssisterIndex === null || row.flashAssisterIndex === undefined)) {
      qaErrors += qaError(`kills.json round ${String(row.roundNumber)} tick ${String(row.tick)}: flashAssist=true requires flashAssisterIndex`);
    }
  }

  // ── columnar stream QA ──────────────────────────────────────────────────
  if (shots) qaErrors += qaShots(shots, nPlayers, roundSet, roundsByNumber);
  if (replay) qaErrors += qaStream("replay.json", asRows(replay.rounds), nPlayers, roundSet, roundsByNumber,
                                   ["x", "y", "z", "yaw", "pitch", "hp", "armor", "money", "equipValue", "weapon", "place", "flash", "flags"]);
  if (duels) qaErrors += qaStream("duels.json", asRows(duels.windows), nPlayers, roundSet, roundsByNumber,
    ["x", "y", "z", "yaw", "pitch", "hp", "flash"]);

  return qaErrors;
}

function qaShots(shots: Row, nPlayers: number, roundSet: Set<number>, roundsByNumber: Map<unknown, Row>): number {
  let qaErrors = 0;
  const weaponDict = Array.isArray(shots.weaponDict) ? shots.weaponDict : [];
  for (const [ti, track] of asRows(shots.tracks).entries()) {
    const label = `shots.json tracks[${ti}]`;
    if (!roundSet.has(Number(track.roundNumber))) {
      qaErrors += qaError(`${label}: roundNumber not in rounds.json`);
      continue;
    }
    const idx = track.playerIndex;
    if (!(typeof idx === "number" && idx >= 0 && idx < nPlayers)) {
      qaErrors += qaError(`${label}: playerIndex out of range`);
    }
    const cols = ["tick", "weapon", "x", "y", "z", "vx", "vy", "vz", "yaw", "pitch"];
    const lengths = new Set(cols.map((c) => (Array.isArray(track[c]) ? (track[c] as unknown[]).length : -1)));
    if (lengths.size > 1) {
      qaErrors += qaError(`${label}: column lengths differ`);
      continue;
    }
    for (const w of (track.weapon as number[] | undefined) ?? []) {
      if (!(w >= 0 && w < weaponDict.length)) {
        qaErrors += qaError(`${label}: weapon index ${w} out of weaponDict range`);
        break;
      }
    }
    const round = roundsByNumber.get(track.roundNumber);
    if (round && Array.isArray(track.tick)) {
      const ticks = decodeDelta(track.tick as number[]);
      const eventEnd = roundEventEnd(roundsByNumber, Number(track.roundNumber));
      if (ticks.some((t) => t < Number(round.freezeEndTick) || t > eventEnd)) {
        qaErrors += qaError(`${label}: decoded ticks fall outside the round window`);
      }
    }
  }
  return qaErrors;
}

function qaStream(name: string, blocks: Row[], nPlayers: number, roundSet: Set<number>,
                  roundsByNumber: Map<unknown, Row>, cols: string[]): number {
  let qaErrors = 0;
  for (const block of blocks) {
    const rn = block.roundNumber;
    const label = `${name} round ${String(rn)}`;
    if (!roundSet.has(Number(rn))) {
      qaErrors += qaError(`${label}: roundNumber not in rounds.json`);
      continue;
    }
    const fc = Number(block.frameCount);
    const round = roundsByNumber.get(rn);
    const start = Number(block.startTick);
    const step = Number(block.tickStep);
    if (round && fc > 0) {
      const last = start + (fc - 1) * step;
      const eventEnd = roundEventEnd(roundsByNumber, Number(rn));
      if (start < Number(round.freezeEndTick) || last > eventEnd) {
        qaErrors += qaError(`${label}: frame grid [${start}, ${last}] outside round window`);
      }
    }
    for (const [pi, track] of asRows(block.players).entries()) {
      const idx = track.playerIndex;
      if (!(typeof idx === "number" && idx >= 0 && idx < nPlayers)) {
        qaErrors += qaError(`${label} players[${pi}]: playerIndex out of range`);
      }
      for (const c of cols) {
        const arr = track[c];
        if (!Array.isArray(arr) || arr.length !== fc) {
          qaErrors += qaError(`${label} players[${pi}]: column "${c}" length != frameCount ${fc}`);
          break;
        }
      }
    }
  }
  return qaErrors;
}

function roundEventEnd(roundsByNumber: Map<unknown, Row>, roundNumber: number): number {
  const round = roundsByNumber.get(roundNumber);
  const nextRound = roundsByNumber.get(roundNumber + 1);
  if (nextRound && typeof nextRound.startTick === "number") {
    return Number(nextRound.startTick) - 1;
  }
  return Number(round?.endTick ?? 0);
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
