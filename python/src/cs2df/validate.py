"""Validate a cs2-demo-format v3 ZIP against the JSON Schemas + package-level QA.

Pure stdlib + jsonschema — works without the demoparser2/pandas stack, so
`cs2df validate` (and the thin tools/validate.py wrapper) run anywhere.
"""

from __future__ import annotations

import json
import math
import re
import sys
import zipfile
from pathlib import Path

REQUIRED_KEYS = {
    "match", "players", "rounds", "playerStats", "playerEconomies",
    "kills", "damages", "blinds", "bombs", "grenades", "clutches",
}
OPTIONAL_KEYS = {"shots", "replay", "duels"}
KNOWN_SCHEMA_VERSIONS = {"cs2-demo-format/3.0"}
EPS = 0.02

# stream track columns that must all share length == frameCount
_REPLAY_COLS = ("x", "y", "z", "yaw", "pitch", "hp", "armor", "money",
                "equipValue", "weapon", "place", "flash", "flags")
_REPLAY_OPTIONAL_FRAME_COLS = ("grenades",)
_DUEL_COLS = ("x", "y", "z", "yaw", "pitch", "hp", "flash")
_SHOT_COLS = ("tick", "weapon", "x", "y", "z", "vx", "vy", "vz", "yaw", "pitch")


def decode_delta(values: list) -> list:
    out = []
    acc = 0
    for v in values:
        acc += v
        out.append(acc)
    return out


def find_invalid_json_values(text: str) -> list[tuple[int, str]]:
    return [(m.start(), m.group()) for m in re.finditer(r"\b(NaN|-?Infinity)\b", text)]


def load_schemas(spec_dir: Path) -> dict:
    schemas = {}
    for f in sorted(spec_dir.glob("*.schema.json")):
        key = f.stem.replace(".schema", "")
        schemas[key] = json.loads(f.read_text())
    return schemas


def validate_zip(zip_path: Path, spec_dir: Path, strict: bool = False) -> bool:
    try:
        import jsonschema  # noqa: F401
    except ImportError:
        print("ERROR: jsonschema not installed. Run: pip install jsonschema", file=sys.stderr)
        sys.exit(1)

    print(f"Validating: {zip_path.name}\n")
    schemas = load_schemas(spec_dir)
    errors: list[str] = []
    warnings: list[str] = []
    data_by_key: dict[str, object] = {}

    def err(msg: str):
        errors.append(msg)
        print(f"  ✗ {msg}")

    def warn(msg: str):
        warnings.append(msg)
        print(f"  ⚠ {msg}")

    def ok(msg: str):
        print(f"  ✓ {msg}")

    def note(msg: str):
        print(f"  • {msg}")

    with zipfile.ZipFile(zip_path) as zf:
        zip_names = set(zf.namelist())
        if "manifest.json" not in zip_names:
            err("manifest.json: missing from ZIP")
            return _finish(errors, warnings, strict)

        manifest = _read_json(zf, "manifest.json", errors)
        if manifest is None:
            return _finish(errors, warnings, strict)
        data_by_key["manifest"] = manifest

        _validate_schema("manifest", manifest, schemas, errors)
        schema_version = manifest.get("schemaVersion", "")
        if schema_version not in KNOWN_SCHEMA_VERSIONS:
            err(f"manifest.json: unsupported schemaVersion '{schema_version}'")

        files_map = manifest.get("files", {}) if isinstance(manifest, dict) else {}
        for key in sorted(REQUIRED_KEYS):
            if key not in files_map:
                err(f"manifest.files: missing required key '{key}'")
        for key in files_map:
            if key not in REQUIRED_KEYS | OPTIONAL_KEYS:
                err(f"manifest.files: unknown key '{key}'")

        for key, filename in files_map.items():
            if filename not in zip_names:
                if key in REQUIRED_KEYS:
                    err(f"{filename}: declared for required key '{key}' but missing from ZIP")
                else:
                    warn(f"{filename}: declared optional key '{key}' but file is missing")
                continue
            value = _read_json(zf, filename, errors)
            if value is None:
                continue
            data_by_key[key] = value
            _validate_schema(key, value, schemas, errors)
            count = len(value) if isinstance(value, list) else 1
            ok(f"{filename} ({count} {'rows' if count != 1 else 'row'})")

    _package_qa(data_by_key, errors, warnings, note)
    print()
    return _finish(errors, warnings, strict)


def _read_json(zf: zipfile.ZipFile, filename: str, errors: list[str]):
    raw = zf.read(filename).decode("utf-8")
    invalid = find_invalid_json_values(raw)
    if invalid:
        sample = ", ".join(f"{v}@{off}" for off, v in invalid[:5])
        errors.append(f"{filename}: invalid JSON value(s): {sample}")
        print(f"  ✗ {filename}: invalid JSON value(s): {sample}")
        return None
    try:
        return json.loads(raw)
    except json.JSONDecodeError as e:
        errors.append(f"{filename}: JSON parse error — {e}")
        print(f"  ✗ {filename}: JSON parse error — {e}")
        return None


def _validate_schema(key: str, value, schemas: dict, errors: list[str]):
    schema = schemas.get(key)
    if schema is None:
        errors.append(f"{key}: no JSON Schema found")
        print(f"  ✗ {key}: no JSON Schema found")
        return
    import jsonschema

    validator = jsonschema.Draft7Validator(schema)
    schema_errors = sorted(validator.iter_errors(value), key=lambda e: list(e.absolute_path))
    for e in schema_errors[:20]:
        path = " → ".join(str(p) for p in e.absolute_path) or "(root)"
        errors.append(f"{key}: [{path}] {e.message}")
        print(f"  ✗ {key}: [{path}] {e.message}")
    if len(schema_errors) > 20:
        errors.append(f"{key}: {len(schema_errors) - 20} additional schema error(s)")
        print(f"  ✗ {key}: {len(schema_errors) - 20} additional schema error(s)")


# ── package-level QA ─────────────────────────────────────────────────────────

def _package_qa(data: dict, errors: list[str], warnings: list[str], note):
    def err(msg: str):
        errors.append(msg)
        print(f"  ✗ {msg}")

    players = _as_list(data.get("players"))
    rounds = _as_list(data.get("rounds"))
    stats = _as_list(data.get("playerStats"))
    economies = _as_list(data.get("playerEconomies"))
    kills = _as_list(data.get("kills"))
    damages = _as_list(data.get("damages"))
    blinds = _as_list(data.get("blinds"))
    bombs = _as_list(data.get("bombs"))
    grenades = _as_list(data.get("grenades"))
    clutches = _as_list(data.get("clutches"))
    shots = data.get("shots") if isinstance(data.get("shots"), dict) else None
    replay = data.get("replay") if isinstance(data.get("replay"), dict) else None
    duels = data.get("duels") if isinstance(data.get("duels"), dict) else None
    match = data.get("match") if isinstance(data.get("match"), dict) else {}

    n_players = len(players)
    team_by_index = {i: p.get("teamKey") for i, p in enumerate(players) if isinstance(p, dict)}
    round_numbers = [r.get("roundNumber") for r in rounds if isinstance(r, dict)]
    round_set = set(round_numbers)
    rounds_by_number = {r.get("roundNumber"): r for r in rounds if isinstance(r, dict)}

    note(f"rows: players={n_players}, rounds={len(rounds)}, kills={len(kills)}, damages={len(damages)}")

    if round_numbers:
        expected = list(range(1, max(round_numbers) + 1))
        if sorted(round_numbers) != expected:
            err("rounds.json: roundNumber must be continuous from 1 with no gaps/duplicates")

    bad_tick_rounds = []
    for r in rounds:
        if not isinstance(r, dict):
            continue
        if r.get("teamASide") == r.get("teamBSide"):
            err(f"rounds.json round {r.get('roundNumber')}: teamASide and teamBSide must differ")
        if not (r.get("startTick", 0) < r.get("freezeEndTick", 0) <= r.get("endTick", 0)):
            bad_tick_rounds.append(r.get("roundNumber"))
    if bad_tick_rounds:
        sample = ", ".join(str(v) for v in bad_tick_rounds[:8])
        err(f"rounds.json: {len(bad_tick_rounds)} row(s) violate tick order start < freezeEnd <= end; sample rounds: {sample}")

    _check_match_score(match, rounds, err)
    _check_round_winner_sides(rounds, err)

    for name, rows in [("kills", kills), ("damages", damages), ("blinds", blinds),
                       ("bombs", bombs), ("grenades", grenades), ("clutches", clutches),
                       ("player-economies", economies)]:
        _check_event_rounds(name, rows, round_set, err)

    _check_tick_windows("kills", kills, rounds_by_number, err, ["tick"])
    _check_tick_windows("damages", damages, rounds_by_number, err, ["tick"])
    _check_tick_windows("blinds", blinds, rounds_by_number, err, ["tick"])
    _check_tick_windows("bombs", bombs, rounds_by_number, err, ["tick"])
    _check_tick_windows("grenades", grenades, rounds_by_number, err,
                        ["throwTick", "effectTick", "destroyTick"])
    _check_tick_windows("clutches", clutches, rounds_by_number, err, ["tick"])

    # playerIndex references must be in range
    for file_name, rows, fields in [
        ("kills", kills, ["killerIndex", "victimIndex", "assisterIndex", "flashAssisterIndex"]),
        ("damages", damages, ["attackerIndex", "victimIndex"]),
        ("blinds", blinds, ["flasherIndex", "flashedIndex"]),
        ("bombs", bombs, ["actorIndex"]),
        ("grenades", grenades, ["throwerIndex"]),
        ("clutches", clutches, ["clutcherIndex"]),
        ("player-economies", economies, ["playerIndex"]),
        ("player-stats", stats, ["playerIndex"]),
    ]:
        _check_player_refs(file_name, rows, fields, n_players, err)

    expected_economies = len(rounds) * n_players
    economy_keys = {(r.get("roundNumber"), r.get("playerIndex")) for r in economies if isinstance(r, dict)}
    if len(economy_keys) != expected_economies:
        err(f"player-economies.json: expected {expected_economies} round/player rows, got {len(economy_keys)} unique rows")

    # damages: cap equation
    for d in damages:
        if not isinstance(d, dict):
            continue
        raw = d.get("healthDamageRaw")
        effective = d.get("healthDamage")
        before = d.get("victimHealthBefore")
        if all(isinstance(v, (int, float)) for v in [raw, effective, before]):
            if effective != min(raw, before):
                err(f"damages.json round {d.get('roundNumber')} tick {d.get('tick')}: healthDamage must equal min(healthDamageRaw, victimHealthBefore)")

    # aggregate damage cross-check (anti-enemy, capped)
    damage_by_player: dict[int, int] = {}
    utility_by_player: dict[int, int] = {}
    utility_weapons = {"hegrenade", "inferno", "molotov", "incendiary"}
    for d in damages:
        if not isinstance(d, dict):
            continue
        atk = d.get("attackerIndex")
        vic = d.get("victimIndex")
        if atk is None or atk == vic:
            continue
        if team_by_index.get(atk) == team_by_index.get(vic):
            continue
        health = d.get("healthDamage")
        if not isinstance(health, int):
            continue
        damage_by_player[atk] = damage_by_player.get(atk, 0) + health
        if d.get("weapon") in utility_weapons:
            utility_by_player[atk] = utility_by_player.get(atk, 0) + health

    for s in stats:
        if not isinstance(s, dict):
            continue
        idx = s.get("playerIndex")
        rounds_count = s.get("rounds")
        label = f"player-stats.json playerIndex={idx}"
        if rounds_count != len(rounds):
            err(f"{label}: rounds must equal rounds.length ({len(rounds)})")
        _expect_equal(s, "damageHealth", damage_by_player.get(idx, 0), err, label)
        _expect_equal(s, "utilityDamage", utility_by_player.get(idx, 0), err, label)
        if isinstance(rounds_count, int) and rounds_count > 0:
            _expect_close(s.get("adr"), s.get("damageHealth", 0) / rounds_count, err, f"{label}: adr")
            _expect_close(s.get("averageUtilityDamagePerRound"), s.get("utilityDamage", 0) / rounds_count, err, f"{label}: averageUtilityDamagePerRound")
            _expect_close(s.get("kast"), s.get("kastRounds", 0) / rounds_count * 100, err, f"{label}: kast")
        for field in ["firstKillCount", "firstDeathCount", "kastRounds", "oneKillCount",
                      "twoKillCount", "threeKillCount", "fourKillCount", "fiveKillCount"]:
            if isinstance(s.get(field), int) and isinstance(rounds_count, int) and s[field] > rounds_count:
                err(f"{label}: {field} cannot exceed rounds")
        for n in ["One", "Two", "Three", "Four", "Five"]:
            count = s.get(f"vs{n}Count")
            won = s.get(f"vs{n}WonCount")
            lost = s.get(f"vs{n}LostCount")
            if all(isinstance(v, int) for v in [count, won, lost]) and won + lost != count:
                err(f"{label}: vs{n}WonCount + vs{n}LostCount must equal vs{n}Count")

    for k in kills:
        if isinstance(k, dict) and k.get("flashAssist") and k.get("flashAssisterIndex") is None:
            err(f"kills.json round {k.get('roundNumber')} tick {k.get('tick')}: flashAssist=true requires flashAssisterIndex")

    for b in bombs:
        if not isinstance(b, dict):
            continue
        if b.get("type") in {"planted", "defused"} and b.get("actorIndex") is None:
            err(f"bombs.json round {b.get('roundNumber')} tick {b.get('tick')}: {b.get('type')} requires actorIndex")
    _check_bomb_lifecycle(bombs, err)

    for g in grenades:
        if isinstance(g, dict) and isinstance(g.get("destroyTick"), int) and g["destroyTick"] < g.get("effectTick", 0):
            err(f"grenades.json round {g.get('roundNumber')} tick {g.get('throwTick')}: destroyTick must be >= effectTick")

    # ── columnar stream QA ────────────────────────────────────────────────
    if shots is not None:
        _check_shots_stream(shots, n_players, round_set, rounds_by_number, err)
    if replay is not None:
        _check_replay_stream(replay, n_players, round_set, rounds_by_number, err)
    if duels is not None:
        _check_duels_stream(duels, n_players, round_set, rounds_by_number, err)


def _check_shots_stream(shots: dict, n_players: int, round_set: set,
                        rounds_by_number: dict, err):
    wd = shots.get("weaponDict", [])
    for ti, t in enumerate(shots.get("tracks", [])):
        if not isinstance(t, dict):
            continue
        label = f"shots.json tracks[{ti}]"
        if t.get("roundNumber") not in round_set:
            err(f"{label}: roundNumber {t.get('roundNumber')} not in rounds.json")
            continue
        if not _index_ok(t.get("playerIndex"), n_players):
            err(f"{label}: playerIndex out of range")
        lengths = {c: len(t.get(c, [])) for c in _SHOT_COLS}
        if len(set(lengths.values())) > 1:
            err(f"{label}: column lengths differ: {lengths}")
            continue
        for w in t.get("weapon", []):
            if not (0 <= w < len(wd)):
                err(f"{label}: weapon index {w} out of weaponDict range")
                break
        ticks = decode_delta(t.get("tick", []))
        rd = rounds_by_number.get(t.get("roundNumber"))
        event_end = _round_event_end(rounds_by_number, t.get("roundNumber"))
        if rd and ticks and event_end is not None and not all(rd.get("freezeEndTick", 0) <= tk <= event_end for tk in ticks):
            err(f"{label}: decoded ticks fall outside the round window")


def _check_track_frames(label: str, track: dict, cols: tuple, frame_count: int,
                        n_players: int, err) -> None:
    if not _index_ok(track.get("playerIndex"), n_players):
        err(f"{label}: playerIndex out of range")
    bad = {c: len(track.get(c, [])) for c in cols if len(track.get(c, [])) != frame_count}
    if bad:
        err(f"{label}: column lengths != frameCount {frame_count}: {bad}")


def _check_replay_stream(replay: dict, n_players: int, round_set: set,
                         rounds_by_number: dict, err):
    wd = replay.get("weaponDict", [])
    pld = replay.get("placeDict", [])
    for rd_obj in replay.get("rounds", []):
        if not isinstance(rd_obj, dict):
            continue
        rn = rd_obj.get("roundNumber")
        label = f"replay.json round {rn}"
        if rn not in round_set:
            err(f"{label}: roundNumber not in rounds.json")
            continue
        fc = rd_obj.get("frameCount", 0)
        rd = rounds_by_number.get(rn, {})
        start = rd_obj.get("startTick", 0)
        step = rd_obj.get("tickStep", 1)
        if fc and rd:
            last_tick = start + (fc - 1) * step
            event_end = _round_event_end(rounds_by_number, rn)
            if event_end is not None and (start < rd.get("freezeEndTick", 0) or last_tick > event_end):
                err(f"{label}: frame grid [{start}, {last_tick}] outside round window")
        for pi, track in enumerate(rd_obj.get("players", [])):
            tlabel = f"{label} players[{pi}]"
            _check_track_frames(tlabel, track, _REPLAY_COLS, fc, n_players, err)
            _check_optional_track_frames(tlabel, track, _REPLAY_OPTIONAL_FRAME_COLS, fc, err)
            for w in track.get("weapon", []):
                if w != -1 and not (0 <= w < len(wd)):
                    err(f"{tlabel}: weapon index {w} out of weaponDict range")
                    break
            for p in track.get("place", []):
                if p != -1 and not (0 <= p < len(pld)):
                    err(f"{tlabel}: place index {p} out of placeDict range")
                    break
        for qi, proj in enumerate(rd_obj.get("projectiles", [])):
            n = len(proj.get("x", []))
            if len(proj.get("y", [])) != n or len(proj.get("z", [])) != n:
                err(f"{label} projectiles[{qi}]: x/y/z lengths differ")
            if proj.get("throwerIndex") is not None and not _index_ok(proj.get("throwerIndex"), n_players):
                err(f"{label} projectiles[{qi}]: throwerIndex out of range")


def _check_optional_track_frames(label: str, track: dict, cols: tuple, frame_count: int, err) -> None:
    bad = {c: len(track.get(c, [])) for c in cols if c in track and len(track.get(c, [])) != frame_count}
    if bad:
        err(f"{label}: optional column lengths != frameCount {frame_count}: {bad}")


def _check_duels_stream(duels: dict, n_players: int, round_set: set,
                        rounds_by_number: dict, err):
    for wi, w in enumerate(duels.get("windows", [])):
        if not isinstance(w, dict):
            continue
        rn = w.get("roundNumber")
        label = f"duels.json windows[{wi}] (round {rn})"
        if rn not in round_set:
            err(f"{label}: roundNumber not in rounds.json")
            continue
        fc = w.get("frameCount", 0)
        rd = rounds_by_number.get(rn, {})
        start = w.get("startTick", 0)
        step = w.get("tickStep", 1)
        if fc and rd:
            last_tick = start + (fc - 1) * step
            event_end = _round_event_end(rounds_by_number, rn)
            if event_end is not None and (start < rd.get("freezeEndTick", 0) or last_tick > event_end):
                err(f"{label}: frame grid [{start}, {last_tick}] outside round window")
        anchors = w.get("anchors", [])
        if not anchors:
            err(f"{label}: empty anchors")
        for a in anchors:
            if not isinstance(a, dict):
                continue
            if not (start <= a.get("tick", 0) <= start + max(0, fc - 1) * step):
                err(f"{label}: anchor tick {a.get('tick')} outside the window")
            if not _index_ok(a.get("victimIndex"), n_players):
                err(f"{label}: anchor victimIndex out of range")
            ai = a.get("attackerIndex")
            if ai is not None and not _index_ok(ai, n_players):
                err(f"{label}: anchor attackerIndex out of range")
        for pi, track in enumerate(w.get("players", [])):
            _check_track_frames(f"{label} players[{pi}]", track, _DUEL_COLS, fc, n_players, err)


# ── shared QA helpers ─────────────────────────────────────────────────────────

def _as_list(value) -> list:
    return value if isinstance(value, list) else []


def _index_ok(idx, n_players: int) -> bool:
    return isinstance(idx, int) and 0 <= idx < n_players


def _check_player_refs(name: str, rows: list, fields: list[str], n_players: int, err):
    for row in rows:
        if not isinstance(row, dict):
            continue
        for field in fields:
            value = row.get(field)
            if value is not None and not _index_ok(value, n_players):
                err(f"{name}.json: {field} {value!r} is not a valid players.json index")


def _check_event_rounds(name: str, rows: list, round_set: set, err):
    missing: dict[object, int] = {}
    for row in rows:
        if isinstance(row, dict) and row.get("roundNumber") not in round_set:
            missing[row.get("roundNumber")] = missing.get(row.get("roundNumber"), 0) + 1
    if missing:
        total = sum(missing.values())
        sample = ", ".join(f"{k} ({v})" for k, v in list(missing.items())[:8])
        err(f"{name}.json: {total} row(s) reference roundNumber not present in rounds.json; sample: {sample}")


def _check_match_score(match: dict, rounds: list, err):
    if not isinstance(match, dict):
        return
    team_a = match.get("teamA") if isinstance(match.get("teamA"), dict) else {}
    team_b = match.get("teamB") if isinstance(match.get("teamB"), dict) else {}
    expected_a = sum(1 for r in rounds if isinstance(r, dict) and r.get("winnerTeamKey") == "teamA")
    expected_b = sum(1 for r in rounds if isinstance(r, dict) and r.get("winnerTeamKey") == "teamB")
    if team_a.get("score") != expected_a or team_b.get("score") != expected_b:
        err(f"match.json: score must equal round winners ({expected_a}:{expected_b})")


def _check_round_winner_sides(rounds: list, err):
    for r in rounds:
        if not isinstance(r, dict):
            continue
        winner = r.get("winnerTeamKey")
        expected = r.get("teamASide") if winner == "teamA" else r.get("teamBSide") if winner == "teamB" else None
        if expected and r.get("winnerSide") != expected:
            err(f"rounds.json round {r.get('roundNumber')}: winnerSide must match winnerTeamKey side")


def _check_tick_windows(name: str, rows: list, rounds_by_number: dict, err,
                        fields: list[str]):
    bad: list[str] = []
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            continue
        round_row = rounds_by_number.get(row.get("roundNumber"))
        if not isinstance(round_row, dict):
            continue
        for field in fields:
            tick = row.get(field)
            if not isinstance(tick, int):
                continue
            start = round_row.get("freezeEndTick")
            end = _round_event_end(rounds_by_number, row.get("roundNumber"))
            if not isinstance(start, int) or not isinstance(end, int):
                continue
            if tick < start or tick > end:
                bad.append(f"row {index} round {row.get('roundNumber')} {field}={tick}")
                break
    if bad:
        sample = "; ".join(bad[:8])
        err(f"{name}.json: {len(bad)} row(s) have ticks outside their round window; sample: {sample}")


def _round_event_end(rounds_by_number: dict, round_number) -> int | None:
    round_row = rounds_by_number.get(round_number)
    if not isinstance(round_row, dict):
        return None
    next_round = rounds_by_number.get(round_number + 1) if isinstance(round_number, int) else None
    if isinstance(next_round, dict) and isinstance(next_round.get("startTick"), int):
        return next_round["startTick"] - 1
    end_tick = round_row.get("endTick")
    return end_tick if isinstance(end_tick, int) else None


def _check_bomb_lifecycle(bombs: list, err):
    by_round: dict[object, list[dict]] = {}
    for b in bombs:
        if isinstance(b, dict):
            by_round.setdefault(b.get("roundNumber"), []).append(b)
    bad: list[str] = []
    for round_number, rows in by_round.items():
        sorted_rows = sorted(rows, key=lambda b: b.get("tick") if isinstance(b.get("tick"), int) else -1)
        planted_tick = None
        for row in sorted_rows:
            event_type = row.get("type")
            tick = row.get("tick")
            if event_type == "planted" and isinstance(tick, int):
                planted_tick = tick
            if event_type in {"exploded", "defused"} and (planted_tick is None or not isinstance(tick, int) or tick < planted_tick):
                bad.append(f"round {round_number} {event_type}@{tick}")
                break
    if bad:
        sample = "; ".join(bad[:8])
        err(f"bombs.json: {len(bad)} round(s) have terminal bomb events before planted; sample: {sample}")


def _expect_equal(row: dict, field: str, expected: int, err, label: str):
    if row.get(field) != expected:
        err(f"{label}: {field} expected {expected}, got {row.get(field)}")


def _expect_close(actual, expected: float, err, label: str):
    if not isinstance(actual, (int, float)) or not math.isfinite(actual) or abs(actual - expected) > EPS:
        err(f"{label} expected {expected:.3f}, got {actual}")


def _finish(errors: list, warnings: list, strict: bool) -> bool:
    effective_errors = len(errors) + (len(warnings) if strict else 0)
    if effective_errors:
        parts = []
        if errors:
            parts.append(f"{len(errors)} error(s)")
        if warnings:
            parts.append(f"{len(warnings)} warning(s)" + (" [strict]" if strict else ""))
        print(f"❌ FAIL — {', '.join(parts)}")
        return False
    if warnings:
        print(f"⚠️  PASS with {len(warnings)} warning(s) (run --strict to treat as errors)")
        return True
    print("✅ PASS")
    return True
