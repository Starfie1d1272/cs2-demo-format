#!/usr/bin/env python3
"""
Validate a cs2-demo-format ZIP against the strict schema and package-level QA.

Usage:
    python tools/validate.py export.zip
    python tools/validate.py export.zip --spec path/to/spec/
    python tools/validate.py export.zip --strict
"""

import argparse
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
OPTIONAL_KEYS = {"shots", "positions1s"}
KNOWN_SCHEMA_VERSIONS = {"cs2-demo-format/2.0"}
ROUND_FILES = REQUIRED_KEYS | OPTIONAL_KEYS - {"match", "players", "playerStats"}
EPS = 0.02


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
        import jsonschema
    except ImportError:
        print("ERROR: jsonschema not installed. Run: pip install jsonschema", file=sys.stderr)
        sys.exit(1)

    print(f"Validating: {zip_path.name}\n")
    schemas = load_schemas(spec_dir)
    errors: list[str] = []
    warnings: list[str] = []
    report: list[str] = []
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
        report.append(msg)
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


def _package_qa(data: dict, errors: list[str], warnings: list[str], note):
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
    shots = _as_list(data.get("shots"))
    positions = _as_list(data.get("positions1s"))
    match = data.get("match") if isinstance(data.get("match"), dict) else {}

    player_ids = {p.get("steamId64") for p in players if isinstance(p, dict)}
    team_by_player = {p.get("steamId64"): p.get("teamKey") for p in players if isinstance(p, dict)}
    round_numbers = [r.get("roundNumber") for r in rounds if isinstance(r, dict)]
    round_set = set(round_numbers)
    rounds_by_number = {r.get("roundNumber"): r for r in rounds if isinstance(r, dict)}

    note(f"rows: players={len(players)}, rounds={len(rounds)}, kills={len(kills)}, damages={len(damages)}")

    if round_numbers:
        expected = list(range(1, max(round_numbers) + 1))
        if sorted(round_numbers) != expected:
            errors.append("rounds.json: roundNumber must be continuous from 1 with no gaps/duplicates")
            print("  ✗ rounds.json: roundNumber must be continuous from 1 with no gaps/duplicates")

    bad_tick_rounds = []
    for r in rounds:
        if not isinstance(r, dict):
            continue
        if r.get("teamASide") == r.get("teamBSide"):
            errors.append(f"rounds.json round {r.get('roundNumber')}: teamASide and teamBSide must differ")
            print(f"  ✗ rounds.json round {r.get('roundNumber')}: teamASide and teamBSide must differ")
        if not (r.get("startTick", 0) < r.get("freezeEndTick", 0) <= r.get("endTick", 0)):
            bad_tick_rounds.append(r.get("roundNumber"))
    if bad_tick_rounds:
        sample = ", ".join(str(v) for v in bad_tick_rounds[:8])
        errors.append(f"rounds.json: {len(bad_tick_rounds)} row(s) violate tick order start < freezeEnd <= end; sample rounds: {sample}")
        print(f"  ✗ rounds.json: {len(bad_tick_rounds)} row(s) violate tick order start < freezeEnd <= end; sample rounds: {sample}")

    _check_match_score(match, rounds, errors)
    _check_round_winner_sides(rounds, errors)

    _check_event_rounds("kills", kills, round_set, errors)
    _check_event_rounds("damages", damages, round_set, errors)
    _check_event_rounds("blinds", blinds, round_set, errors)
    _check_event_rounds("bombs", bombs, round_set, errors)
    _check_event_rounds("grenades", grenades, round_set, errors)
    _check_event_rounds("clutches", clutches, round_set, errors)
    _check_event_rounds("shots", shots, round_set, errors)
    _check_event_rounds("positions-1s", positions, round_set, errors)

    _check_tick_windows("kills", kills, rounds_by_number, errors, [("tick", False)])
    _check_tick_windows("damages", damages, rounds_by_number, errors, [("tick", False)])
    _check_tick_windows("blinds", blinds, rounds_by_number, errors, [("tick", False)])
    _check_tick_windows("bombs", bombs, rounds_by_number, errors, [("tick", False)])
    _check_tick_windows("grenades", grenades, rounds_by_number, errors, [("throwTick", False), ("effectTick", False), ("destroyTick", False)])
    _check_tick_windows("clutches", clutches, rounds_by_number, errors, [("tick", False)])
    _check_tick_windows("shots", shots, rounds_by_number, errors, [("tick", False)])
    _check_tick_windows("positions-1s", positions, rounds_by_number, errors, [("tick", False)])

    for file_name, rows, fields in [
        ("kills", kills, ["killerSteamId64", "victimSteamId64", "assisterSteamId64", "flashAssisterSteamId64"]),
        ("damages", damages, ["attackerSteamId64", "victimSteamId64"]),
        ("blinds", blinds, ["flasherSteamId64", "flashedSteamId64"]),
        ("bombs", bombs, ["actorSteamId64"]),
        ("grenades", grenades, ["throwerSteamId64"]),
        ("clutches", clutches, ["clutcherSteamId64"]),
        ("playerEconomies", economies, ["steamId64"]),
        ("playerStats", stats, ["steamId64"]),
    ]:
        _check_steam_refs(file_name, rows, fields, player_ids, errors)

    _check_team_refs(kills, team_by_player, errors, "kills", [
        ("killerSteamId64", "killerTeamKey"),
        ("victimSteamId64", "victimTeamKey"),
    ])
    _check_team_refs(damages, team_by_player, errors, "damages", [
        ("attackerSteamId64", "attackerTeamKey"),
        ("victimSteamId64", "victimTeamKey"),
    ])

    expected_economies = len(rounds) * len(players)
    economy_keys = {(r.get("roundNumber"), r.get("steamId64")) for r in economies if isinstance(r, dict)}
    if len(economy_keys) != expected_economies:
        errors.append(f"player-economies.json: expected {expected_economies} round/player rows, got {len(economy_keys)} unique rows")
        print(f"  ✗ player-economies.json: expected {expected_economies} round/player rows, got {len(economy_keys)} unique rows")

    for d in damages:
        if not isinstance(d, dict):
            continue
        raw = d.get("healthDamageRaw")
        effective = d.get("healthDamage")
        before = d.get("victimHealthBefore")
        if all(isinstance(v, (int, float)) for v in [raw, effective, before]):
            expected = min(raw, before)
            if effective != expected:
                errors.append(f"damages.json round {d.get('roundNumber')} tick {d.get('tick')}: healthDamage must equal min(healthDamageRaw, victimHealthBefore)")
                print(f"  ✗ damages.json round {d.get('roundNumber')} tick {d.get('tick')}: healthDamage must equal min(healthDamageRaw, victimHealthBefore)")

    damage_by_player: dict[str, int] = {}
    utility_by_player: dict[str, int] = {}
    utility_weapons = {"hegrenade", "inferno", "molotov", "incendiary"}
    for d in damages:
        if not isinstance(d, dict):
            continue
        attacker = d.get("attackerSteamId64")
        if attacker is None or d.get("attackerTeamKey") == d.get("victimTeamKey"):
            continue
        health = d.get("healthDamage")
        if not isinstance(health, int):
            continue
        damage_by_player[attacker] = damage_by_player.get(attacker, 0) + health
        if d.get("weapon") in utility_weapons:
            utility_by_player[attacker] = utility_by_player.get(attacker, 0) + health

    for s in stats:
        if not isinstance(s, dict):
            continue
        sid = s.get("steamId64")
        rounds_count = s.get("rounds")
        if rounds_count != len(rounds):
            errors.append(f"player-stats.json {sid}: rounds must equal rounds.length ({len(rounds)})")
            print(f"  ✗ player-stats.json {sid}: rounds must equal rounds.length ({len(rounds)})")
        _expect_equal(s, "damageHealth", damage_by_player.get(sid, 0), errors, f"player-stats.json {sid}")
        _expect_equal(s, "utilityDamage", utility_by_player.get(sid, 0), errors, f"player-stats.json {sid}")
        if isinstance(rounds_count, int) and rounds_count > 0:
            _expect_close(s.get("adr"), s.get("damageHealth", 0) / rounds_count, errors, f"player-stats.json {sid}: adr")
            _expect_close(s.get("averageUtilityDamagePerRound"), s.get("utilityDamage", 0) / rounds_count, errors, f"player-stats.json {sid}: averageUtilityDamagePerRound")
            _expect_close(s.get("kast"), s.get("kast_rounds", 0) / rounds_count * 100, errors, f"player-stats.json {sid}: kast")
        for field in ["firstKillCount", "firstDeathCount", "kast_rounds", "oneKillCount", "twoKillCount", "threeKillCount", "fourKillCount", "fiveKillCount"]:
            if isinstance(s.get(field), int) and isinstance(rounds_count, int) and s[field] > rounds_count:
                errors.append(f"player-stats.json {sid}: {field} cannot exceed rounds")
                print(f"  ✗ player-stats.json {sid}: {field} cannot exceed rounds")
        for n in ["One", "Two", "Three", "Four", "Five"]:
            count = s.get(f"vs{n}Count")
            won = s.get(f"vs{n}WonCount")
            lost = s.get(f"vs{n}LostCount")
            if all(isinstance(v, int) for v in [count, won, lost]) and won + lost != count:
                errors.append(f"player-stats.json {sid}: vs{n}WonCount + vs{n}LostCount must equal vs{n}Count")
                print(f"  ✗ player-stats.json {sid}: vs{n}WonCount + vs{n}LostCount must equal vs{n}Count")

    for k in kills:
        if isinstance(k, dict) and k.get("flashAssist") and not k.get("flashAssisterSteamId64"):
            errors.append(f"kills.json round {k.get('roundNumber')} tick {k.get('tick')}: flashAssist=true requires flashAssisterSteamId64")
            print(f"  ✗ kills.json round {k.get('roundNumber')} tick {k.get('tick')}: flashAssist=true requires flashAssisterSteamId64")

    for b in bombs:
        if not isinstance(b, dict):
            continue
        if b.get("type") in {"planted", "defused"} and not b.get("actorSteamId64"):
            errors.append(f"bombs.json round {b.get('roundNumber')} tick {b.get('tick')}: {b.get('type')} requires actorSteamId64")
            print(f"  ✗ bombs.json round {b.get('roundNumber')} tick {b.get('tick')}: {b.get('type')} requires actorSteamId64")
    _check_bomb_lifecycle(bombs, errors)

    for g in grenades:
        if isinstance(g, dict) and isinstance(g.get("destroyTick"), int) and g["destroyTick"] < g.get("effectTick", 0):
            errors.append(f"grenades.json round {g.get('roundNumber')} tick {g.get('throwTick')}: destroyTick must be >= effectTick")
            print(f"  ✗ grenades.json round {g.get('roundNumber')} tick {g.get('throwTick')}: destroyTick must be >= effectTick")


def _as_list(value) -> list:
    return value if isinstance(value, list) else []


def _check_event_rounds(name: str, rows: list, round_set: set, errors: list[str]):
    missing: dict[object, int] = {}
    for row in rows:
        if isinstance(row, dict) and row.get("roundNumber") not in round_set:
            missing[row.get("roundNumber")] = missing.get(row.get("roundNumber"), 0) + 1
    if missing:
        total = sum(missing.values())
        sample = ", ".join(f"{k} ({v})" for k, v in list(missing.items())[:8])
        errors.append(f"{name}.json: {total} row(s) reference roundNumber not present in rounds.json; sample: {sample}")
        print(f"  ✗ {name}.json: {total} row(s) reference roundNumber not present in rounds.json; sample: {sample}")


def _check_match_score(match: dict, rounds: list, errors: list[str]):
    if not isinstance(match, dict):
        return
    team_a = match.get("teamA") if isinstance(match.get("teamA"), dict) else {}
    team_b = match.get("teamB") if isinstance(match.get("teamB"), dict) else {}
    expected_a = sum(1 for r in rounds if isinstance(r, dict) and r.get("winnerTeamKey") == "teamA")
    expected_b = sum(1 for r in rounds if isinstance(r, dict) and r.get("winnerTeamKey") == "teamB")
    if team_a.get("score") != expected_a or team_b.get("score") != expected_b:
        errors.append(f"match.json: score must equal round winners ({expected_a}:{expected_b})")
        print(f"  ✗ match.json: score must equal round winners ({expected_a}:{expected_b})")


def _check_round_winner_sides(rounds: list, errors: list[str]):
    for r in rounds:
        if not isinstance(r, dict):
            continue
        winner = r.get("winnerTeamKey")
        expected = r.get("teamASide") if winner == "teamA" else r.get("teamBSide") if winner == "teamB" else None
        if expected and r.get("winnerSide") != expected:
            errors.append(f"rounds.json round {r.get('roundNumber')}: winnerSide must match winnerTeamKey side")
            print(f"  ✗ rounds.json round {r.get('roundNumber')}: winnerSide must match winnerTeamKey side")


def _check_tick_windows(
    name: str,
    rows: list,
    rounds_by_number: dict,
    errors: list[str],
    fields: list[tuple[str, bool]],
):
    bad: list[str] = []
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            continue
        round_row = rounds_by_number.get(row.get("roundNumber"))
        if not isinstance(round_row, dict):
            continue
        for field, allow_freeze in fields:
            tick = row.get(field)
            if tick is None:
                continue
            if not isinstance(tick, int):
                continue
            start = round_row.get("startTick") if allow_freeze else round_row.get("freezeEndTick")
            end = round_row.get("endTick")
            if not isinstance(start, int) or not isinstance(end, int):
                continue
            if tick < start or tick > end:
                bad.append(f"row {index} round {row.get('roundNumber')} {field}={tick}")
                break
    if bad:
        sample = "; ".join(bad[:8])
        errors.append(f"{name}.json: {len(bad)} row(s) have ticks outside their round window; sample: {sample}")
        print(f"  ✗ {name}.json: {len(bad)} row(s) have ticks outside their round window; sample: {sample}")


def _check_bomb_lifecycle(bombs: list, errors: list[str]):
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
        errors.append(f"bombs.json: {len(bad)} round(s) have terminal bomb events before planted; sample: {sample}")
        print(f"  ✗ bombs.json: {len(bad)} round(s) have terminal bomb events before planted; sample: {sample}")


def _check_steam_refs(name: str, rows: list, fields: list[str], player_ids: set, errors: list[str]):
    for row in rows:
        if not isinstance(row, dict):
            continue
        for field in fields:
            value = row.get(field)
            if value is not None and value not in player_ids:
                errors.append(f"{name}.json: {field} '{value}' is not present in players.json")
                print(f"  ✗ {name}.json: {field} '{value}' is not present in players.json")


def _check_team_refs(name_rows: list, team_by_player: dict, errors: list[str], name: str, pairs: list[tuple[str, str]]):
    for row in name_rows:
        if not isinstance(row, dict):
            continue
        for id_field, team_field in pairs:
            sid = row.get(id_field)
            team = row.get(team_field)
            if sid is not None and team is not None and team_by_player.get(sid) != team:
                errors.append(f"{name}.json: {team_field} does not match players.teamKey for {sid}")
                print(f"  ✗ {name}.json: {team_field} does not match players.teamKey for {sid}")


def _expect_equal(row: dict, field: str, expected: int, errors: list[str], label: str):
    if row.get(field) != expected:
        errors.append(f"{label}: {field} expected {expected}, got {row.get(field)}")
        print(f"  ✗ {label}: {field} expected {expected}, got {row.get(field)}")


def _expect_close(actual, expected: float, errors: list[str], label: str):
    if not isinstance(actual, (int, float)) or not math.isfinite(actual) or abs(actual - expected) > EPS:
        errors.append(f"{label} expected {expected:.3f}, got {actual}")
        print(f"  ✗ {label} expected {expected:.3f}, got {actual}")


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


def main():
    parser = argparse.ArgumentParser(description="Validate a CS2 demo export ZIP")
    parser.add_argument("zip", help="Path to the .zip file to validate")
    parser.add_argument("--spec", default=None, help="Path to spec/ directory")
    parser.add_argument("--strict", action="store_true", help="Treat warnings as errors")
    args = parser.parse_args()

    zip_path = Path(args.zip)
    if not zip_path.exists():
        print(f"ERROR: file not found: {zip_path}", file=sys.stderr)
        sys.exit(1)

    spec_dir = Path(args.spec) if args.spec else Path(__file__).parent.parent / "spec"
    if not spec_dir.exists():
        print(f"ERROR: spec directory not found: {spec_dir}", file=sys.stderr)
        sys.exit(1)

    ok = validate_zip(zip_path, spec_dir, strict=args.strict)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
