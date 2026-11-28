/**
 * Forward-compatibility & multi-version tests for parser/index.ts.
 *
 * Verifies:
 * 1. Parsing a 3.0 ZIP built from fixtures/v3-mid/.
 * 2. Parsing a 3.1 ZIP with 3.1 optional fields & weapons.json.
 * 3. Parsing a synthetic 3.2 ZIP with unknown top-level/nested fields and
 *    unregistered manifest.files keys (forward compatibility).
 * 4. All FLAG_* constants are exported and distinct powers of two.
 */

import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import JSZip from 'jszip';
import {
  parseDemoPackage,
  decodeTrackFrames,
  FLAG_ALIVE,
  FLAG_HAS_BOMB,
  FLAG_HAS_DEFUSE_KIT,
  FLAG_HAS_HELMET,
  FLAG_IS_SCOPED,
  FLAG_IS_WALKING,
  FLAG_IS_AIRBORNE,
  FLAG_IS_DUCKING,
  FLAG_IS_IN_RELOAD,
} from '../parser/index.js';

async function buildZipFromFixture(dir: string): Promise<Buffer> {
  const zip = new JSZip();
  for (const name of readdirSync(dir)) {
    if (name.endsWith('.json')) {
      zip.file(name, readFileSync(join(dir, name), 'utf-8'));
    }
  }
  return zip.generateAsync({ type: 'nodebuffer' });
}

async function main() {
  // 0. Check FLAG_* constants
  const flags = [
    FLAG_ALIVE,
    FLAG_HAS_BOMB,
    FLAG_HAS_DEFUSE_KIT,
    FLAG_HAS_HELMET,
    FLAG_IS_SCOPED,
    FLAG_IS_WALKING,
    FLAG_IS_AIRBORNE,
    FLAG_IS_DUCKING,
    FLAG_IS_IN_RELOAD,
  ];
  assert.deepEqual(flags, [1, 2, 4, 8, 16, 32, 64, 128, 256]);

  // 1. 3.0 fixture ZIP
  const fixtureDir = join(process.cwd(), 'fixtures', 'v3-mid');
  const buf30 = await buildZipFromFixture(fixtureDir);
  const pkg30 = await parseDemoPackage(buf30);
  assert.equal(pkg30.manifest.schemaVersion, 'cs2-demo-format/3.0');
  assert.equal(pkg30.files.players.length, 10);
  assert.equal(pkg30.files.rounds.length, 21);
  assert.ok(pkg30.files.replay);
  const firstRound = pkg30.files.replay.rounds[0];
  const decoded = decodeTrackFrames(firstRound.players[0], firstRound, pkg30.files.replay.meta);
  assert.equal(decoded.length, firstRound.frameCount);

  // 2. 3.1 synthetic ZIP (upgraded from v3-mid with 3.1 fields + weapons.json)
  const zip31 = await JSZip.loadAsync(buf30);
  const manifest31 = JSON.parse(await zip31.file('manifest.json')!.async('text'));
  manifest31.schemaVersion = 'cs2-demo-format/3.1';
  manifest31.files.weapons = 'weapons.json';
  zip31.file('manifest.json', JSON.stringify(manifest31));

  const players31 = JSON.parse(await zip31.file('players.json')!.async('text'));
  players31[0].crosshairCode = 'CSGO-s5Qbj-nvF89-cJjDd-mRdSG-5Yt4N';
  zip31.file('players.json', JSON.stringify(players31));

  const kills31 = JSON.parse(await zip31.file('kills.json')!.async('text'));
  kills31[0].killerBlinded = true;
  zip31.file('kills.json', JSON.stringify(kills31));

  const grenades31 = JSON.parse(await zip31.file('grenades.json')!.async('text'));
  grenades31[0].firePoints = [{ x: 100, y: 200, z: 10 }];
  zip31.file('grenades.json', JSON.stringify(grenades31));

  zip31.file(
    'weapons.json',
    JSON.stringify([
      {
        roundNumber: 1,
        entityId: 42,
        weapon: 'ak47',
        startTick: firstRound.startTick + 10,
        endTick: firstRound.startTick + 120,
        previousOwnerIndex: 0,
        startPosition: { x: 10, y: 20, z: 30 },
        endPosition: { x: 12, y: 22, z: 30 },
      },
    ]),
  );

  const buf31 = await zip31.generateAsync({ type: 'nodebuffer' });
  const pkg31 = await parseDemoPackage(buf31);
  assert.equal(pkg31.manifest.schemaVersion, 'cs2-demo-format/3.1');
  assert.equal(pkg31.files.players[0].crosshairCode, 'CSGO-s5Qbj-nvF89-cJjDd-mRdSG-5Yt4N');
  assert.equal(pkg31.files.kills[0].killerBlinded, true);
  assert.equal(pkg31.files.grenades[0].firePoints?.length, 1);
  assert.equal(pkg31.files.weapons?.length, 1);

  // 3. 3.2 forward-compatibility ZIP (unknown fields, nested unknown fields, unknown file key)
  const zip32 = await JSZip.loadAsync(buf31);
  const manifest32 = JSON.parse(await zip32.file('manifest.json')!.async('text'));
  manifest32.schemaVersion = 'cs2-demo-format/3.2';
  manifest32.futureTopLevel = 'ignored';
  manifest32.exporter.futureNested = true;
  manifest32.files.futureAnalytics = 'future-analytics.json';
  zip32.file('manifest.json', JSON.stringify(manifest32));
  zip32.file('future-analytics.json', JSON.stringify({ experimental: [1, 2, 3] }));

  const match32 = JSON.parse(await zip32.file('match.json')!.async('text'));
  match32.futureField = 42;
  match32.teamA.futureNested = 'ignored';
  zip32.file('match.json', JSON.stringify(match32));

  const buf32 = await zip32.generateAsync({ type: 'nodebuffer' });
  const pkg32 = await parseDemoPackage(buf32);
  assert.equal(pkg32.manifest.schemaVersion, 'cs2-demo-format/3.2');
  assert.equal(pkg32.files.match.mapName, 'de_anubis');

  // 4. Preserves array .min(1) constraint on duels.anchors even when loosened
  const zipBadDuels = await JSZip.loadAsync(buf31);
  const duelsBad = JSON.parse(await zipBadDuels.file('duels.json')!.async('text'));
  duelsBad.windows[0].anchors = [];
  zipBadDuels.file('duels.json', JSON.stringify(duelsBad));
  const bufBadDuels = await zipBadDuels.generateAsync({ type: 'nodebuffer' });
  await assert.rejects(() => parseDemoPackage(bufBadDuels));

  console.log('✅ parser/index.ts passed 3.0, 3.1, and 3.2 forward-compatibility tests');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
