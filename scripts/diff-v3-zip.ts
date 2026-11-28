import fs from 'node:fs';
import JSZip from 'jszip';

const FLOAT_TOL = 0.02;
const SAMPLE_LIMIT = 3;

interface PlayerRow {
  steamId64: string;
  name: string;
  teamKey: string;
}

type JsonMap = Record<string, unknown>;

interface FieldDiffBucket {
  count: number;
  maxAbsDiff: number | null;
  samples: string[];
}

async function loadZipJson(path: string): Promise<Record<string, unknown>> {
  const buf = fs.readFileSync(path);
  const zip = await JSZip.loadAsync(buf);
  const out: Record<string, unknown> = {};
  for (const [name, entry] of Object.entries(zip.files)) {
    if (!entry.dir && name.endsWith('.json')) {
      const text = await entry.async('string');
      out[name] = JSON.parse(text);
    }
  }
  return out;
}

function decodeDelta(arr: number[]): number[] {
  const out: number[] = [];
  let acc = 0;
  for (const v of arr) {
    acc += v;
    out.push(acc);
  }
  return out;
}

function resolvePlayer(players: PlayerRow[], idx: unknown): string | null {
  if (typeof idx !== 'number' || idx < 0 || idx >= players.length) return null;
  return players[idx].steamId64;
}

function resolveDict(dict: string[], idx: unknown): string | null {
  if (typeof idx !== 'number' || idx < 0 || idx >= dict.length) return null;
  return dict[idx];
}

function omitKeys(obj: JsonMap, keysToOmit: ReadonlySet<string>): JsonMap {
  if (keysToOmit.size === 0) return { ...obj };
  const out: JsonMap = {};
  for (const [k, v] of Object.entries(obj)) {
    if (!keysToOmit.has(k)) out[k] = v;
  }
  return out;
}

function normalizeRows(
  rows: JsonMap[],
  players: PlayerRow[],
  playerFields: string[],
  sortKeys: string[],
  omitFields: ReadonlySet<string> = new Set(),
): JsonMap[] {
  const mapped = rows.map((r) => {
    const copy = omitKeys(r, omitFields);
    for (const pf of playerFields) {
      if (pf in copy) {
        copy[pf] = resolvePlayer(players, copy[pf]);
      }
    }
    return copy;
  });
  mapped.sort((a, b) => {
    for (const k of sortKeys) {
      const av = String(a[k] ?? '');
      const bv = String(b[k] ?? '');
      if (av !== bv) return av < bv ? -1 : 1;
    }
    return 0;
  });
  return mapped;
}

function normalizeReplay(
  replay: JsonMap | undefined,
  players: PlayerRow[],
  v30Compat: boolean,
): unknown {
  if (!replay) return null;
  const weaponDict = (replay.weaponDict as string[]) ?? [];
  const placeDict = (replay.placeDict as string[]) ?? [];
  const rounds = ((replay.rounds as JsonMap[]) ?? []).map((rd) => {
    const pTracks = ((rd.players as JsonMap[]) ?? [])
      .map((t) => {
        const rawFlags = (t.flags as number[] | undefined) ?? [];
        const flags = v30Compat ? rawFlags.map((f) => f & 7) : rawFlags;
        const base: JsonMap = {
          playerSid: resolvePlayer(players, t.playerIndex),
          x: decodeDelta((t.x as number[]) ?? []),
          y: decodeDelta((t.y as number[]) ?? []),
          z: decodeDelta((t.z as number[]) ?? []),
          yaw: decodeDelta((t.yaw as number[]) ?? []),
          pitch: decodeDelta((t.pitch as number[]) ?? []),
          hp: t.hp,
          armor: t.armor,
          money: decodeDelta((t.money as number[]) ?? []),
          equipValue: decodeDelta((t.equipValue as number[]) ?? []),
          weapon: ((t.weapon as number[]) ?? []).map((i) => resolveDict(weaponDict, i)),
          place: ((t.place as number[]) ?? []).map((i) => resolveDict(placeDict, i)),
          flash: t.flash,
          flags,
          grenades: t.grenades,
        };
        if (!v30Compat && 'ammo' in t) {
          base.ammo = t.ammo;
        }
        return base;
      })
      .sort((a, b) => String(a.playerSid ?? '').localeCompare(String(b.playerSid ?? '')));

    const projectiles = ((rd.projectiles as JsonMap[]) ?? [])
      .map((p) => ({
        grenade: p.grenade,
        throwerSid: resolvePlayer(players, p.throwerIndex),
        startTick: p.startTick,
        x: decodeDelta((p.x as number[]) ?? []),
        y: decodeDelta((p.y as number[]) ?? []),
        z: decodeDelta((p.z as number[]) ?? []),
      }))
      .sort(
        (a, b) =>
          Number(a.startTick) - Number(b.startTick) ||
          String(a.grenade).localeCompare(String(b.grenade)) ||
          String(a.throwerSid ?? '').localeCompare(String(b.throwerSid ?? '')),
      );

    return {
      roundNumber: rd.roundNumber,
      startTick: rd.startTick,
      tickStep: rd.tickStep,
      frameCount: rd.frameCount,
      players: pTracks,
      projectiles,
    };
  });
  return { meta: replay.meta, rounds };
}

function normalizeDuels(
  duels: JsonMap | undefined,
  players: PlayerRow[],
  v30Compat: boolean,
): unknown {
  if (!duels) return null;
  const windows = ((duels.windows as JsonMap[]) ?? []).map((w) => ({
    roundNumber: w.roundNumber,
    startTick: w.startTick,
    tickStep: w.tickStep,
    frameCount: w.frameCount,
    anchors: ((w.anchors as JsonMap[]) ?? []).map((a) => ({
      kind: a.kind,
      tick: a.tick,
      attackerSid: resolvePlayer(players, a.attackerIndex),
      victimSid: resolvePlayer(players, a.victimIndex),
    })),
    players: ((w.players as JsonMap[]) ?? [])
      .map((t) => {
        const base: JsonMap = {
          playerSid: resolvePlayer(players, t.playerIndex),
          x: decodeDelta((t.x as number[]) ?? []),
          y: decodeDelta((t.y as number[]) ?? []),
          z: decodeDelta((t.z as number[]) ?? []),
          yaw: decodeDelta((t.yaw as number[]) ?? []),
          pitch: decodeDelta((t.pitch as number[]) ?? []),
          hp: t.hp,
          flash: t.flash,
        };
        if (!v30Compat && 'flags' in t) {
          base.flags = t.flags;
        }
        return base;
      })
      .sort((a, b) => String(a.playerSid ?? '').localeCompare(String(b.playerSid ?? ''))),
  }));
  return { meta: duels.meta, windows };
}

function normalizeShots(
  shots: JsonMap | undefined,
  players: PlayerRow[],
  v30Compat: boolean,
): unknown {
  if (!shots) return null;
  const weaponDict = (shots.weaponDict as string[]) ?? [];
  const tracks = ((shots.tracks as JsonMap[]) ?? [])
    .map((t) => {
      const base: JsonMap = {
        roundNumber: t.roundNumber,
        playerSid: resolvePlayer(players, t.playerIndex),
        tick: decodeDelta((t.tick as number[]) ?? []),
        weapon: ((t.weapon as number[]) ?? []).map((i) => resolveDict(weaponDict, i)),
        x: decodeDelta((t.x as number[]) ?? []),
        y: decodeDelta((t.y as number[]) ?? []),
        z: decodeDelta((t.z as number[]) ?? []),
        vx: t.vx,
        vy: t.vy,
        vz: t.vz,
        yaw: decodeDelta((t.yaw as number[]) ?? []),
        pitch: decodeDelta((t.pitch as number[]) ?? []),
      };
      if (!v30Compat) {
        for (const k of ['accuracyPenalty', 'recoilIdx', 'ammo', 'flags']) {
          if (k in t) base[k] = t[k];
        }
      }
      return base;
    })
    .sort(
      (a, b) =>
        Number(a.roundNumber) - Number(b.roundNumber) ||
        String(a.playerSid ?? '').localeCompare(String(b.playerSid ?? '')),
    );
  return { meta: shots.meta, tracks };
}

function normalizeFieldPath(path: string): string {
  return path.replace(/\[\d+\]/g, '[*]');
}

function recordDiff(
  buckets: Map<string, FieldDiffBucket>,
  rawPath: string,
  message: string,
  absDiff: number | null,
): void {
  const key = normalizeFieldPath(rawPath);
  let bucket = buckets.get(key);
  if (!bucket) {
    bucket = { count: 0, maxAbsDiff: null, samples: [] };
    buckets.set(key, bucket);
  }
  bucket.count += 1;
  if (absDiff !== null) {
    bucket.maxAbsDiff = bucket.maxAbsDiff === null ? absDiff : Math.max(bucket.maxAbsDiff, absDiff);
  }
  if (bucket.samples.length < SAMPLE_LIMIT) {
    bucket.samples.push(`${rawPath}: ${message}`);
  }
}

function diffValues(
  a: unknown,
  b: unknown,
  path: string,
  buckets: Map<string, FieldDiffBucket>,
): void {
  if (a === b) return;
  if (typeof a === 'number' && typeof b === 'number') {
    const absDiff = Math.abs(a - b);
    if (absDiff <= FLOAT_TOL) return;
    recordDiff(buckets, path, `${a} !== ${b}`, absDiff);
    return;
  }
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) {
      recordDiff(buckets, `${path}.length`, `${a.length} !== ${b.length}`, Math.abs(a.length - b.length));
      return;
    }
    for (let i = 0; i < a.length; i++) {
      diffValues(a[i], b[i], `${path}[${i}]`, buckets);
    }
    return;
  }
  if (a && b && typeof a === 'object' && typeof b === 'object') {
    const ao = a as JsonMap;
    const bo = b as JsonMap;
    const keys = Array.from(new Set([...Object.keys(ao), ...Object.keys(bo)])).sort();
    for (const k of keys) {
      diffValues(ao[k], bo[k], `${path}.${k}`, buckets);
    }
    return;
  }
  recordDiff(buckets, path, `${JSON.stringify(a)} !== ${JSON.stringify(b)}`, null);
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  let v30VsV31 = false;
  let quiet = false;
  const positional: string[] = [];
  for (const arg of args) {
    if (arg === '--v30-vs-v31') {
      v30VsV31 = true;
    } else if (arg === '--quiet') {
      quiet = true;
    } else {
      positional.push(arg);
    }
  }
  const [aPath, bPath] = positional;
  if (!aPath || !bPath) {
    console.error('Usage: tsx scripts/diff-v3-zip.ts [--v30-vs-v31] [--quiet] <a.zip> <b.zip>');
    process.exit(2);
  }

  const [aFiles, bFiles] = await Promise.all([loadZipJson(aPath), loadZipJson(bPath)]);
  const aPlayers = (aFiles['players.json'] as PlayerRow[]) ?? [];
  const bPlayers = (bFiles['players.json'] as PlayerRow[]) ?? [];

  const aManifest = aFiles['manifest.json'] as JsonMap | undefined;
  const bManifest = bFiles['manifest.json'] as JsonMap | undefined;
  if (
    !v30VsV31 &&
    aManifest?.schemaVersion === 'cs2-demo-format/3.0' &&
    bManifest?.schemaVersion === 'cs2-demo-format/3.1'
  ) {
    v30VsV31 = true;
  }

  let totalDiffs = 0;

  const check = (name: string, a: unknown, b: unknown) => {
    const buckets = new Map<string, FieldDiffBucket>();
    diffValues(a, b, name, buckets);
    if (buckets.size === 0) {
      if (!quiet) console.log(`  ✓ ${name}: exact semantic match`);
      return;
    }
    let fileDiffs = 0;
    for (const bucket of buckets.values()) fileDiffs += bucket.count;
    totalDiffs += fileDiffs;
    console.log(`  ✗ ${name}: ${fileDiffs} diff(s) across ${buckets.size} field(s)`);
    for (const [fieldPath, bucket] of Array.from(buckets.entries()).sort((x, y) => x[0].localeCompare(y[0]))) {
      const maxStr = bucket.maxAbsDiff !== null ? ` (maxAbsDiff=${bucket.maxAbsDiff})` : '';
      console.log(`      ${fieldPath}: ${bucket.count}${maxStr}`);
      for (const s of bucket.samples) {
        console.log(`        · ${s}`);
      }
    }
  };

  // manifest (ignoring exportedAt, exporter, parser; in 3.0-vs-3.1 also schemaVersion and files.weapons)
  const normManifest = (m: JsonMap | undefined) => {
    if (!m) return null;
    const files = v30VsV31
      ? omitKeys((m.files as JsonMap) ?? {}, new Set(['weapons']))
      : m.files;
    return {
      schemaVersion: v30VsV31 ? 'cs2-demo-format/3.x' : m.schemaVersion,
      demo: m.demo,
      mapName: m.mapName,
      tickrate: m.tickrate,
      files,
    };
  };

  const normPlayers = (rows: PlayerRow[]) =>
    rows
      .map((p) => (v30VsV31 ? omitKeys(p as unknown as JsonMap, new Set(['crosshairCode'])) : p))
      .sort((a, b) => String((a as JsonMap).steamId64).localeCompare(String((b as JsonMap).steamId64)));

  const filterBombs = (rows: JsonMap[]) =>
    v30VsV31
      ? rows.filter((r) => r.type !== 'plant_abort' && r.type !== 'defuse_abort')
      : rows;

  check('manifest.json', normManifest(aManifest), normManifest(bManifest));
  check('match.json', aFiles['match.json'], bFiles['match.json']);
  check('players.json', normPlayers(aPlayers), normPlayers(bPlayers));
  check('rounds.json', aFiles['rounds.json'], bFiles['rounds.json']);
  check(
    'player-stats.json',
    normalizeRows(
      (aFiles['player-stats.json'] as JsonMap[]) ?? [],
      aPlayers,
      ['playerIndex'],
      ['playerIndex'],
    ),
    normalizeRows(
      (bFiles['player-stats.json'] as JsonMap[]) ?? [],
      bPlayers,
      ['playerIndex'],
      ['playerIndex'],
    ),
  );
  check(
    'player-economies.json',
    normalizeRows(
      (aFiles['player-economies.json'] as JsonMap[]) ?? [],
      aPlayers,
      ['playerIndex'],
      ['roundNumber', 'playerIndex'],
    ),
    normalizeRows(
      (bFiles['player-economies.json'] as JsonMap[]) ?? [],
      bPlayers,
      ['playerIndex'],
      ['roundNumber', 'playerIndex'],
    ),
  );
  check(
    'kills.json',
    normalizeRows(
      (aFiles['kills.json'] as JsonMap[]) ?? [],
      aPlayers,
      ['killerIndex', 'victimIndex', 'assisterIndex', 'flashAssisterIndex'],
      ['roundNumber', 'tick', 'victimIndex'],
      v30VsV31 ? new Set(['killerBlinded']) : new Set(),
    ),
    normalizeRows(
      (bFiles['kills.json'] as JsonMap[]) ?? [],
      bPlayers,
      ['killerIndex', 'victimIndex', 'assisterIndex', 'flashAssisterIndex'],
      ['roundNumber', 'tick', 'victimIndex'],
      v30VsV31 ? new Set(['killerBlinded']) : new Set(),
    ),
  );
  check(
    'damages.json',
    normalizeRows(
      (aFiles['damages.json'] as JsonMap[]) ?? [],
      aPlayers,
      ['attackerIndex', 'victimIndex'],
      ['roundNumber', 'tick', 'victimIndex', 'attackerIndex'],
    ),
    normalizeRows(
      (bFiles['damages.json'] as JsonMap[]) ?? [],
      bPlayers,
      ['attackerIndex', 'victimIndex'],
      ['roundNumber', 'tick', 'victimIndex', 'attackerIndex'],
    ),
  );
  check(
    'blinds.json',
    normalizeRows(
      (aFiles['blinds.json'] as JsonMap[]) ?? [],
      aPlayers,
      ['flasherIndex', 'flashedIndex'],
      ['roundNumber', 'tick', 'flashedIndex', 'flasherIndex'],
    ),
    normalizeRows(
      (bFiles['blinds.json'] as JsonMap[]) ?? [],
      bPlayers,
      ['flasherIndex', 'flashedIndex'],
      ['roundNumber', 'tick', 'flashedIndex', 'flasherIndex'],
    ),
  );
  check(
    'bombs.json',
    normalizeRows(
      filterBombs((aFiles['bombs.json'] as JsonMap[]) ?? []),
      aPlayers,
      ['actorIndex'],
      ['roundNumber', 'tick', 'type'],
    ),
    normalizeRows(
      filterBombs((bFiles['bombs.json'] as JsonMap[]) ?? []),
      bPlayers,
      ['actorIndex'],
      ['roundNumber', 'tick', 'type'],
    ),
  );
  check(
    'grenades.json',
    normalizeRows(
      (aFiles['grenades.json'] as JsonMap[]) ?? [],
      aPlayers,
      ['throwerIndex'],
      ['roundNumber', 'throwTick', 'effectTick', 'grenade', 'throwerIndex'],
      v30VsV31 ? new Set(['firePoints']) : new Set(),
    ),
    normalizeRows(
      (bFiles['grenades.json'] as JsonMap[]) ?? [],
      bPlayers,
      ['throwerIndex'],
      ['roundNumber', 'throwTick', 'effectTick', 'grenade', 'throwerIndex'],
      v30VsV31 ? new Set(['firePoints']) : new Set(),
    ),
  );
  check(
    'clutches.json',
    normalizeRows(
      (aFiles['clutches.json'] as JsonMap[]) ?? [],
      aPlayers,
      ['clutcherIndex'],
      ['roundNumber', 'tick', 'clutcherIndex'],
    ),
    normalizeRows(
      (bFiles['clutches.json'] as JsonMap[]) ?? [],
      bPlayers,
      ['clutcherIndex'],
      ['roundNumber', 'tick', 'clutcherIndex'],
    ),
  );
  check(
    'shots.json',
    normalizeShots(aFiles['shots.json'] as JsonMap | undefined, aPlayers, v30VsV31),
    normalizeShots(bFiles['shots.json'] as JsonMap | undefined, bPlayers, v30VsV31),
  );
  check(
    'replay.json',
    normalizeReplay(aFiles['replay.json'] as JsonMap | undefined, aPlayers, v30VsV31),
    normalizeReplay(bFiles['replay.json'] as JsonMap | undefined, bPlayers, v30VsV31),
  );
  check(
    'duels.json',
    normalizeDuels(aFiles['duels.json'] as JsonMap | undefined, aPlayers, v30VsV31),
    normalizeDuels(bFiles['duels.json'] as JsonMap | undefined, bPlayers, v30VsV31),
  );

  process.exit(totalDiffs === 0 ? 0 : 1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
