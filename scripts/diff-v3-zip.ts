import fs from 'node:fs';
import JSZip from 'jszip';

const FLOAT_TOL = 0.02;

interface PlayerRow {
  steamId64: string;
  name: string;
  teamKey: string;
}

type JsonMap = Record<string, unknown>;

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

function normalizeRows(
  rows: JsonMap[],
  players: PlayerRow[],
  playerFields: string[],
  sortKeys: string[],
): JsonMap[] {
  const mapped = rows.map((r) => {
    const copy: JsonMap = { ...r };
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

function normalizeReplay(replay: JsonMap | undefined, players: PlayerRow[]): unknown {
  if (!replay) return null;
  const weaponDict = (replay.weaponDict as string[]) ?? [];
  const placeDict = (replay.placeDict as string[]) ?? [];
  const rounds = ((replay.rounds as JsonMap[]) ?? []).map((rd) => {
    const pTracks = ((rd.players as JsonMap[]) ?? [])
      .map((t) => ({
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
        flags: t.flags,
        grenades: t.grenades,
      }))
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

function normalizeDuels(duels: JsonMap | undefined, players: PlayerRow[]): unknown {
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
      .map((t) => ({
        playerSid: resolvePlayer(players, t.playerIndex),
        x: decodeDelta((t.x as number[]) ?? []),
        y: decodeDelta((t.y as number[]) ?? []),
        z: decodeDelta((t.z as number[]) ?? []),
        yaw: decodeDelta((t.yaw as number[]) ?? []),
        pitch: decodeDelta((t.pitch as number[]) ?? []),
        hp: t.hp,
        flash: t.flash,
      }))
      .sort((a, b) => String(a.playerSid ?? '').localeCompare(String(b.playerSid ?? ''))),
  }));
  return { meta: duels.meta, windows };
}

function normalizeShots(shots: JsonMap | undefined, players: PlayerRow[]): unknown {
  if (!shots) return null;
  const weaponDict = (shots.weaponDict as string[]) ?? [];
  const tracks = ((shots.tracks as JsonMap[]) ?? [])
    .map((t) => ({
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
    }))
    .sort(
      (a, b) =>
        Number(a.roundNumber) - Number(b.roundNumber) ||
        String(a.playerSid ?? '').localeCompare(String(b.playerSid ?? '')),
    );
  return { meta: shots.meta, tracks };
}

function diffValues(a: unknown, b: unknown, path: string, diffs: string[], limit = 20): void {
  if (diffs.length >= limit) return;
  if (a === b) return;
  if (typeof a === 'number' && typeof b === 'number') {
    if (Math.abs(a - b) <= FLOAT_TOL) return;
    diffs.push(`${path}: ${a} !== ${b}`);
    return;
  }
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) {
      diffs.push(`${path}.length: ${a.length} !== ${b.length}`);
      return;
    }
    for (let i = 0; i < a.length; i++) {
      diffValues(a[i], b[i], `${path}[${i}]`, diffs, limit);
      if (diffs.length >= limit) return;
    }
    return;
  }
  if (a && b && typeof a === 'object' && typeof b === 'object') {
    const ao = a as JsonMap;
    const bo = b as JsonMap;
    const keys = Array.from(new Set([...Object.keys(ao), ...Object.keys(bo)])).sort();
    for (const k of keys) {
      diffValues(ao[k], bo[k], `${path}.${k}`, diffs, limit);
      if (diffs.length >= limit) return;
    }
    return;
  }
  diffs.push(`${path}: ${JSON.stringify(a)} !== ${JSON.stringify(b)}`);
}

async function main(): Promise<void> {
  const [pyPath, rustPath] = process.argv.slice(2);
  if (!pyPath || !rustPath) {
    console.error('Usage: tsx scripts/diff-v3-zip.ts <py.zip> <rust.zip>');
    process.exit(2);
  }

  const [pyFiles, rustFiles] = await Promise.all([loadZipJson(pyPath), loadZipJson(rustPath)]);
  const pyPlayers = (pyFiles['players.json'] as PlayerRow[]) ?? [];
  const rustPlayers = (rustFiles['players.json'] as PlayerRow[]) ?? [];

  let totalDiffs = 0;

  const check = (name: string, a: unknown, b: unknown) => {
    const diffs: string[] = [];
    diffValues(a, b, name, diffs);
    if (diffs.length === 0) {
      console.log(`  ✓ ${name}: exact semantic match`);
    } else {
      totalDiffs += diffs.length;
      console.log(`  ✗ ${name}: ${diffs.length}+ diff(s)`);
      for (const d of diffs.slice(0, 10)) {
        console.log(`      ${d}`);
      }
    }
  };

  // manifest (ignoring exportedAt, exporter, parser)
  const normManifest = (m: JsonMap | undefined) =>
    m
      ? {
          schemaVersion: m.schemaVersion,
          demo: m.demo,
          mapName: m.mapName,
          tickrate: m.tickrate,
          files: m.files,
        }
      : null;

  check(
    'manifest.json',
    normManifest(pyFiles['manifest.json'] as JsonMap),
    normManifest(rustFiles['manifest.json'] as JsonMap),
  );
  check('match.json', pyFiles['match.json'], rustFiles['match.json']);
  check(
    'players.json',
    [...pyPlayers].sort((a, b) => a.steamId64.localeCompare(b.steamId64)),
    [...rustPlayers].sort((a, b) => a.steamId64.localeCompare(b.steamId64)),
  );
  check('rounds.json', pyFiles['rounds.json'], rustFiles['rounds.json']);
  check(
    'player-stats.json',
    normalizeRows(
      (pyFiles['player-stats.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['playerIndex'],
      ['playerIndex'],
    ),
    normalizeRows(
      (rustFiles['player-stats.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['playerIndex'],
      ['playerIndex'],
    ),
  );
  check(
    'player-economies.json',
    normalizeRows(
      (pyFiles['player-economies.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['playerIndex'],
      ['roundNumber', 'playerIndex'],
    ),
    normalizeRows(
      (rustFiles['player-economies.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['playerIndex'],
      ['roundNumber', 'playerIndex'],
    ),
  );
  check(
    'kills.json',
    normalizeRows(
      (pyFiles['kills.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['killerIndex', 'victimIndex', 'assisterIndex', 'flashAssisterIndex'],
      ['roundNumber', 'tick', 'victimIndex'],
    ),
    normalizeRows(
      (rustFiles['kills.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['killerIndex', 'victimIndex', 'assisterIndex', 'flashAssisterIndex'],
      ['roundNumber', 'tick', 'victimIndex'],
    ),
  );
  check(
    'damages.json',
    normalizeRows(
      (pyFiles['damages.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['attackerIndex', 'victimIndex'],
      ['roundNumber', 'tick', 'victimIndex', 'attackerIndex'],
    ),
    normalizeRows(
      (rustFiles['damages.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['attackerIndex', 'victimIndex'],
      ['roundNumber', 'tick', 'victimIndex', 'attackerIndex'],
    ),
  );
  check(
    'blinds.json',
    normalizeRows(
      (pyFiles['blinds.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['flasherIndex', 'flashedIndex'],
      ['roundNumber', 'tick', 'flashedIndex', 'flasherIndex'],
    ),
    normalizeRows(
      (rustFiles['blinds.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['flasherIndex', 'flashedIndex'],
      ['roundNumber', 'tick', 'flashedIndex', 'flasherIndex'],
    ),
  );
  check(
    'bombs.json',
    normalizeRows(
      (pyFiles['bombs.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['actorIndex'],
      ['roundNumber', 'tick', 'type'],
    ),
    normalizeRows(
      (rustFiles['bombs.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['actorIndex'],
      ['roundNumber', 'tick', 'type'],
    ),
  );
  check(
    'grenades.json',
    normalizeRows(
      (pyFiles['grenades.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['throwerIndex'],
      ['roundNumber', 'throwTick', 'effectTick', 'grenade', 'throwerIndex'],
    ),
    normalizeRows(
      (rustFiles['grenades.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['throwerIndex'],
      ['roundNumber', 'throwTick', 'effectTick', 'grenade', 'throwerIndex'],
    ),
  );
  check(
    'clutches.json',
    normalizeRows(
      (pyFiles['clutches.json'] as JsonMap[]) ?? [],
      pyPlayers,
      ['clutcherIndex'],
      ['roundNumber', 'tick', 'clutcherIndex'],
    ),
    normalizeRows(
      (rustFiles['clutches.json'] as JsonMap[]) ?? [],
      rustPlayers,
      ['clutcherIndex'],
      ['roundNumber', 'tick', 'clutcherIndex'],
    ),
  );
  check(
    'shots.json',
    normalizeShots(pyFiles['shots.json'] as JsonMap | undefined, pyPlayers),
    normalizeShots(rustFiles['shots.json'] as JsonMap | undefined, rustPlayers),
  );
  check(
    'replay.json',
    normalizeReplay(pyFiles['replay.json'] as JsonMap | undefined, pyPlayers),
    normalizeReplay(rustFiles['replay.json'] as JsonMap | undefined, rustPlayers),
  );
  check(
    'duels.json',
    normalizeDuels(pyFiles['duels.json'] as JsonMap | undefined, pyPlayers),
    normalizeDuels(rustFiles['duels.json'] as JsonMap | undefined, rustPlayers),
  );

  process.exit(totalDiffs === 0 ? 0 : 1);
}

main().catch((err) => {
  console.error(err);
  process.exit(2);
});
