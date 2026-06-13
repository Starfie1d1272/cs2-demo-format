"""v3 event-file builders (row-oriented JSON files).

All files reference players by `playerIndex` (row index into players.json).
Per-row teamKey/side fields were removed in v3 — they are derivable from
playerIndex + rounds.json — but team/side VALIDITY is still enforced here so
warmup or unresolvable rows never leak into the package.

Provenance: ported from cs2-demo-analysis-kit (originally DrEAmSs59/
CS2-insight-agent, with the author's permission) and reshaped for v3.
"""

from __future__ import annotations

import math
import re
from collections import defaultdict

from .enums import (
    normalize_hitgroup, classify_inventory, normalize_weapon_name,
    bomb_site_from_place, _BOMB_TYPE_MAP, _GRENADE_TYPE_ENUM,
)
from .rounds import _RoundModel, _event_steamid

_STEAMID_RE = re.compile(r"^\d{17}$")


# ── helper primitives ─────────────────────────────────────────────────────────

def _safe_float(val, default: float = 0.0) -> float:
    if val is None:
        return default
    try:
        f = float(val)
        return default if (math.isnan(f) or math.isinf(f)) else f
    except (TypeError, ValueError):
        return default


def _safe_float_nullable(val) -> float | None:
    if val is None:
        return None
    try:
        f = float(val)
        return None if (math.isnan(f) or math.isinf(f)) else f
    except (TypeError, ValueError):
        return None


def _sid(val) -> str | None:
    s = str(val or "").strip()
    if s.endswith(".0"):
        s = s[:-2]
    return s if s and s not in ("0", "nan", "None") else None


def _is_valid_steamid(s) -> bool:
    return isinstance(s, str) and bool(_STEAMID_RE.match(s))


def _is_valid_side(s) -> bool:
    return s in ("t", "ct")


def _is_valid_teamkey(s) -> bool:
    return s in ("teamA", "teamB")


def _raw(row: dict, k: str):
    return row.get(k) if row.get(k) is not None else row.get(k.lower())


def _pos(row: dict, xk="X", yk="Y", zk="Z") -> dict:
    """Non-nullable integer vec3; NaN/missing → 0."""
    return {
        "x": int(round(_safe_float(_raw(row, xk)))),
        "y": int(round(_safe_float(_raw(row, yk)))),
        "z": int(round(_safe_float(_raw(row, zk)))),
    }


def _pos_nullable(row: dict, xk="X", yk="Y", zk="Z") -> dict | None:
    xv = _safe_float_nullable(_raw(row, xk))
    yv = _safe_float_nullable(_raw(row, yk))
    zv = _safe_float_nullable(_raw(row, zk))
    if xv is None and yv is None and zv is None:
        return None
    return {"x": int(round(xv or 0.0)), "y": int(round(yv or 0.0)), "z": int(round(zv or 0.0))}


def _b(val) -> bool:
    if isinstance(val, bool):
        return val
    try:
        return int(val or 0) != 0
    except (TypeError, ValueError):
        return False


def _event_entity_id(row: dict) -> int | None:
    for key in ("entityid", "entity_id", "grenade_entity_id"):
        val = row.get(key)
        if val is None:
            continue
        try:
            return int(val)
        except (TypeError, ValueError):
            continue
    return None


def _event_round_number(round_model: _RoundModel, row: dict) -> int | None:
    n = round_model.round_for_event(row)
    if n is None:
        return None
    return n if round_model.has_round(n) else None


def _active_event_round_number(round_model: _RoundModel, row: dict) -> int | None:
    n = _event_round_number(round_model, row)
    if n is None:
        return None
    tick = int(row.get("tick") or 0)
    window = round_model.window_for_round(n)
    event_end = round_model.event_end_tick(n)
    if window is None or event_end is None or tick < window.freeze_end_tick or tick > event_end:
        return None
    return n


class PlayerDirectory:
    """players.json rows + sid → (playerIndex, teamKey) lookups."""

    def __init__(self, rows: list[dict]):
        self.rows = rows
        self.index_by_sid: dict[str, int] = {r["steamId64"]: i for i, r in enumerate(rows)}
        self.team_by_sid: dict[str, str] = {r["steamId64"]: r["teamKey"] for r in rows}

    def idx(self, sid: str | None) -> int | None:
        if sid is None:
            return None
        return self.index_by_sid.get(sid)

    def team(self, sid: str | None) -> str | None:
        if sid is None:
            return None
        return self.team_by_sid.get(sid)

    def team_of_index(self, idx: int) -> str:
        return self.rows[idx]["teamKey"]


# ── players / match ───────────────────────────────────────────────────────────

def build_players(raw: dict) -> PlayerDirectory:
    team_num_to_key = {2: "teamA", 3: "teamB"}
    seen: set[str] = set()
    out: list[dict] = []
    for r in raw.get("player_info", []):
        sid = _sid(r.get("steamid"))
        if not sid or sid in seen or not _is_valid_steamid(sid):
            continue
        seen.add(sid)
        team_key = team_num_to_key.get(int(r.get("team_num") or 0))
        if not team_key:
            continue
        out.append({"steamId64": sid, "name": str(r.get("name") or sid), "teamKey": team_key})
    # Stable, deterministic order: teamA before teamB, then by steamId64.
    out.sort(key=lambda p: (p["teamKey"], p["steamId64"]))
    return PlayerDirectory(out)


def build_match(raw: dict, rounds: list[dict]) -> dict:
    hdr = raw.get("header", {})
    team_a_score = sum(1 for r in rounds if r["winnerTeamKey"] == "teamA")
    team_b_score = sum(1 for r in rounds if r["winnerTeamKey"] == "teamB")
    team_a_name = (raw.get("team_a_name") or str(hdr.get("team_name_t") or "")).strip() or None
    team_b_name = (raw.get("team_b_name") or str(hdr.get("team_name_ct") or "")).strip() or None

    duration = _safe_float(hdr.get("playback_time"), default=0.0)
    if not duration:
        last_tick = max((r["endTick"] for r in rounds if r.get("endTick")), default=0)
        tickrate = max(int(raw.get("tickrate") or 64), 1)
        duration = round(last_tick / tickrate, 1)
    if not duration or duration <= 0:
        duration = 1.0

    return {
        "mapName": str(hdr.get("map_name") or "unknown"),
        "tickrate": raw.get("tickrate", 64),
        "durationSeconds": duration,
        "serverName": str(hdr.get("server_name") or "").strip() or None,
        "source": "demo",
        "teamA": {"teamKey": "teamA", "name": team_a_name, "score": team_a_score},
        "teamB": {"teamKey": "teamB", "name": team_b_name, "score": team_b_score},
    }


# ── kills ─────────────────────────────────────────────────────────────────────

def build_kills(raw: dict, players: PlayerDirectory, round_model: _RoundModel) -> list[dict]:
    out = []
    for r in raw.get("deaths", []):
        n = _active_event_round_number(round_model, r)
        if n is None:
            continue
        victim_sid = _sid(r.get("user_steamid"))
        victim_idx = players.idx(victim_sid)
        if victim_idx is None:
            continue
        # victim must resolve to a formal side this round
        if not _is_valid_side(round_model.side_map.get((n, players.team(victim_sid)), "unknown")):
            continue
        weapon = str(r.get("weapon") or "")
        if not weapon:
            continue

        killer_sid = _sid(r.get("attacker_steamid"))
        killer_idx = players.idx(killer_sid)
        assist_idx = players.idx(_sid(r.get("assister_steamid")))
        flash_assist = _b(r.get("assistedflash"))
        flash_assister_idx = assist_idx if flash_assist else None

        out.append({
            "roundNumber": n,
            "tick": int(r.get("tick") or 0),
            "killerIndex": killer_idx,
            "victimIndex": victim_idx,
            "assisterIndex": assist_idx,
            "flashAssisterIndex": flash_assister_idx,
            "weapon": weapon,
            "killerActiveWeapon": normalize_weapon_name(r.get("attacker_active_weapon")),
            "victimActiveWeapon": normalize_weapon_name(r.get("user_active_weapon")),
            "headshot": _b(r.get("headshot")),
            "flashAssist": flash_assist and flash_assister_idx is not None,
            "tradeKill": False,
            "tradeDeath": False,
            "throughSmoke": _b(r.get("thrusmoke")),
            "noScope": _b(r.get("noscope")),
            "penetratedObjects": int(r.get("penetrated_objects") or r.get("penetrated") or 0),
            "killerPosition": _pos_nullable(r, "attacker_X", "attacker_Y", "attacker_Z"),
            "victimPosition": _pos(r, "user_X", "user_Y", "user_Z"),
        })
    _annotate_trades(out)
    return out


def _annotate_trades(kills: list[dict], trade_window_ticks: int = 384) -> None:
    """Mark tradeKill / tradeDeath within a rolling 6-second window (384 ticks at 64hz)."""
    for i, kill in enumerate(kills):
        if kill["killerIndex"] is None:
            continue
        for j in range(i - 1, max(i - 20, -1), -1):
            prev = kills[j]
            if kill["tick"] - prev["tick"] > trade_window_ticks:
                break
            if prev["killerIndex"] == kill["victimIndex"]:
                kills[i]["tradeKill"] = True
                kills[j]["tradeDeath"] = True
                break


# ── damages ───────────────────────────────────────────────────────────────────

def build_damages(raw: dict, players: PlayerDirectory, round_model: _RoundModel) -> list[dict]:
    out: list[dict] = []
    remaining_health: dict[tuple[int, int], int] = defaultdict(lambda: 100)

    for r in sorted(raw.get("hurts", []), key=lambda row: int(row.get("tick") or 0)):
        n = round_model.round_for_event(r)
        if n is None:
            continue
        vic_sid = _sid(r.get("user_steamid"))
        vic_idx = players.idx(vic_sid)
        if vic_idx is None or not _is_valid_steamid(vic_sid):
            continue
        if not _is_valid_side(round_model.side_map.get((n, players.team(vic_sid)), "unknown")):
            continue
        weapon = str(r.get("weapon") or "")
        if not weapon:
            continue

        atk_idx = players.idx(_sid(r.get("attacker_steamid")))

        raw_dmg = int(r.get("dmg_health") or 0)
        health_key = (n, vic_idx)
        health_before = remaining_health[health_key]
        health_dmg = min(max(raw_dmg, 0), health_before)
        remaining_health[health_key] = health_before - health_dmg
        armor_after = min(int(r.get("armor") or 0), 100)

        out.append({
            "roundNumber": n,
            "tick": int(r.get("tick") or 0),
            "attackerIndex": atk_idx,
            "victimIndex": vic_idx,
            "weapon": weapon,
            "hitgroup": normalize_hitgroup(r.get("hitgroup")),
            "healthDamage": health_dmg,
            "healthDamageRaw": raw_dmg,
            "armorDamage": int(r.get("dmg_armor") or 0),
            "victimHealthBefore": health_before,
            "victimArmorAfter": armor_after,
            "attackerPosition": _pos_nullable(r, "attacker_X", "attacker_Y", "attacker_Z"),
            "victimPosition": _pos(r, "user_X", "user_Y", "user_Z"),
        })
    return out


# ── blinds ────────────────────────────────────────────────────────────────────

def build_blinds(raw: dict, players: PlayerDirectory, round_model: _RoundModel,
                 flash_lookup: dict | None = None) -> list[dict]:
    flash_lookup = flash_lookup or {}
    out = []
    for r in raw.get("blinds", []):
        n = round_model.round_for_event(r)
        if n is None:
            continue
        flasher_sid = _sid(r.get("attacker_steamid"))
        flashed_sid = _sid(r.get("user_steamid"))
        flasher_idx = players.idx(flasher_sid)
        flashed_idx = players.idx(flashed_sid)
        if flasher_idx is None or flashed_idx is None:
            continue
        if not _is_valid_side(round_model.side_map.get((n, players.team(flasher_sid)), "unknown")):
            continue
        if not _is_valid_side(round_model.side_map.get((n, players.team(flashed_sid)), "unknown")):
            continue

        dur = min(_safe_float(r.get("blind_duration") or r.get("duration"), default=0.0), 6.0)
        tick = int(r.get("tick") or 0)
        out.append({
            "roundNumber": n,
            "tick": tick,
            "flashId": flash_lookup.get((n, tick)),
            "flasherIndex": flasher_idx,
            "flashedIndex": flashed_idx,
            "durationSeconds": round(dur, 3),
        })
    return out


# ── bombs ─────────────────────────────────────────────────────────────────────

def build_bombs(raw: dict, players: PlayerDirectory, round_model: _RoundModel) -> list[dict]:
    out = []
    # A/B from the actor's last_place_name at plant; reused for defuse/explode.
    round_site: dict[int, str] = {}
    for r in raw.get("bomb_planted", []):
        n = round_model.round_for_event(r)
        site = bomb_site_from_place(r.get("user_last_place_name"))
        if n is not None and site is not None:
            round_site[n] = site
    _ROUND_SITE_TYPES = {"planted", "defused", "exploded", "defuse_begin"}

    event_sources = [
        ("bomb_planted", "planted"),
        ("bomb_defused", "defused"),
        ("bomb_exploded", "exploded"),
        ("bomb_beginplant", "plant"),
        ("bomb_begindefuse", "defuse"),
        ("bomb_dropped", "dropped"),
        ("bomb_pickup", "picked_up"),
    ]

    for rows_key, ev_type in event_sources:
        v3_type = _BOMB_TYPE_MAP.get(ev_type)
        if v3_type is None:
            continue
        for r in raw.get(rows_key, []):
            n = round_model.round_for_event(r)
            if n is None:
                continue
            tick = int(r.get("tick") or 0)
            window = round_model.window_for_round(n)
            event_end = round_model.event_end_tick(n)
            if window is None or event_end is None or tick < window.freeze_end_tick or tick > event_end:
                continue
            actor_sid = _sid(r.get("user_steamid") or r.get("steamid") or r.get("userid"))
            site = bomb_site_from_place(r.get("user_last_place_name"))
            if site is None and v3_type in _ROUND_SITE_TYPES:
                site = round_site.get(n)
            out.append({
                "roundNumber": n,
                "tick": tick,
                "type": v3_type,
                "site": site,
                "actorIndex": players.idx(actor_sid),
                "position": _pos(r, "user_X", "user_Y", "user_Z"),
            })
    out.sort(key=lambda x: (x["roundNumber"], x["tick"]))
    return out


# ── grenades ─────────────────────────────────────────────────────────────────

def build_grenades(raw: dict, players: PlayerDirectory, round_model: _RoundModel) -> list[dict]:
    throws: list[dict] = []
    for r in raw.get("grenade_throws", []):
        tick = int(r.get("tick") or 0)
        n = round_model.round_for_tick(tick)
        if n is None:
            continue
        gtype = str(r.get("grenade") or "")
        if gtype not in _GRENADE_TYPE_ENUM:
            continue
        destroy_tick = int(r.get("destroy_tick") or 0)
        eid = r.get("grenade_entity_id")
        gid = f"{int(eid)}-{tick}" if eid is not None else None
        throws.append({
            "rn": n,
            "tick": tick,
            "destroy_tick": destroy_tick if destroy_tick > 0 else None,
            "gtype": gtype,
            "sid": _event_steamid(r),
            "eid": gid,
            "pos": _pos(r),
        })
    throws.sort(key=lambda t: t["tick"])

    def _match_throw(round_num: int, gtype: str, effect_tick: int, thrower_sid: str | None) -> dict | None:
        pool = [
            t for t in throws
            if t["rn"] == round_num and t["gtype"] == gtype and t["tick"] <= effect_tick
            and (thrower_sid is None or t["sid"] == thrower_sid)
        ]
        return max(pool, key=lambda t: t["tick"]) if pool else None

    out = []
    for r in raw.get("grenade_detonations", []):
        n = round_model.round_for_event(r)
        if n is None:
            continue
        tick = int(r.get("tick") or 0)
        window = round_model.window_for_round(n)
        event_end = round_model.event_end_tick(n)
        if window is None or event_end is None or tick < window.freeze_end_tick or tick > event_end:
            continue
        gtype = str(r.get("_grenade_type") or "")
        if gtype not in _GRENADE_TYPE_ENUM:
            continue

        thrower_sid = _event_steamid(r)
        matched = _match_throw(n, gtype, tick, thrower_sid)
        detonate_entity_id = _event_entity_id(r)
        if matched:
            thrower_sid = thrower_sid or matched["sid"]
            throw_pos = matched["pos"]
            throw_tick = matched["tick"]
            destroy_tick = None if gtype == "smoke" else matched["destroy_tick"]
            grenade_id = matched["eid"]
        else:
            throw_pos = _pos(r)
            throw_tick = tick
            destroy_tick = None
            grenade_id = f"{detonate_entity_id}-{tick}" if detonate_entity_id is not None else None

        if throw_tick <= 0 or tick <= 0:
            continue
        thrower_idx = players.idx(thrower_sid)
        if thrower_idx is None:
            continue
        if not _is_valid_side(round_model.side_map.get((n, players.team(thrower_sid)), "unknown")):
            continue
        if destroy_tick is not None and (
            destroy_tick < tick or event_end is None or destroy_tick > event_end
        ):
            destroy_tick = None

        out.append({
            "roundNumber": n,
            "grenadeId": grenade_id,
            "throwTick": throw_tick,
            "effectTick": tick,
            "destroyTick": destroy_tick,
            "_entityId": detonate_entity_id,
            "grenade": gtype,
            "throwerIndex": thrower_idx,
            "throwPosition": throw_pos,
            "effectPosition": _pos(r),
        })

    # molotov burn end: pair with the nearest later inferno_expire in-round.
    expires: list[dict] = []
    for r in raw.get("inferno_expires", []):
        n = round_model.round_for_event(r)
        t = int(r.get("tick") or 0)
        if n is None or t <= 0:
            continue
        expires.append({"rn": n, "tick": t, "pos": _pos(r), "used": False})
    expires.sort(key=lambda e: e["tick"])
    for g in out:
        if g["grenade"] != "molotov" or g["destroyTick"] is not None:
            continue
        event_end = round_model.event_end_tick(g["roundNumber"])
        best = None
        best_d2 = None
        for e in expires:
            if e["used"] or e["rn"] != g["roundNumber"] or e["tick"] < g["effectTick"]:
                continue
            if event_end is not None and e["tick"] > event_end:
                continue
            ep, gp = e["pos"], g["effectPosition"]
            d2 = (ep["x"] - gp["x"]) ** 2 + (ep["y"] - gp["y"]) ** 2
            if best is None or d2 < best_d2:
                best, best_d2 = e, d2
        if best is not None:
            best["used"] = True
            g["destroyTick"] = best["tick"]

    # smoke lifetime from smokegrenade_expired (same entity id).
    smoke_index: dict[tuple[int, int], list[dict]] = {}
    for r in raw.get("smoke_expires", []):
        n = round_model.round_for_event(r)
        t = int(r.get("tick") or 0)
        eid = _event_entity_id(r)
        if n is None or t <= 0 or eid is None:
            continue
        smoke_index.setdefault((n, eid), []).append({"tick": t})
    for lst in smoke_index.values():
        lst.sort(key=lambda e: e["tick"])

    for g in out:
        eid = g.pop("_entityId", None)
        if g["grenade"] != "smoke" or eid is None:
            continue
        event_end = round_model.event_end_tick(g["roundNumber"])
        for e in smoke_index.get((g["roundNumber"], eid), []):
            if e["tick"] < g["effectTick"]:
                continue
            if event_end is not None and e["tick"] > event_end:
                continue
            g["destroyTick"] = e["tick"]
            break
    return out


# ── clutches ──────────────────────────────────────────────────────────────────

def build_clutches(kills: list[dict], rounds: list[dict],
                   players: PlayerDirectory) -> list[dict]:
    """Detect 1vN situations: one alive player vs N enemies at some point in the round."""
    out: list[dict] = []
    rounds_by_n = {r["roundNumber"]: r for r in rounds}

    kills_by_round: dict[int, list[dict]] = {}
    for k in kills:
        kills_by_round.setdefault(k["roundNumber"], []).append(k)

    team_indexes: dict[str, set[int]] = {"teamA": set(), "teamB": set()}
    for i, p in enumerate(players.rows):
        team_indexes[p["teamKey"]].add(i)

    for rn, rnd in rounds_by_n.items():
        rnd_kills = sorted(kills_by_round.get(rn, []), key=lambda x: x["tick"])
        if not rnd_kills:
            continue

        clutch_detected: set[int] = set()
        dead: set[int] = set()
        for k in rnd_kills:
            dead.add(k["victimIndex"])
            a_alive = team_indexes["teamA"] - dead
            b_alive = team_indexes["teamB"] - dead

            for team_key, own_alive, opp_alive in (
                ("teamA", a_alive, b_alive),
                ("teamB", b_alive, a_alive),
            ):
                if len(own_alive) != 1 or len(opp_alive) < 1:
                    continue
                idx = next(iter(own_alive))
                if idx in clutch_detected:
                    continue
                clutch_detected.add(idx)
                remaining_kills = sum(
                    1 for kk in rnd_kills
                    if kk["tick"] >= k["tick"] and kk["killerIndex"] == idx
                )
                out.append({
                    "roundNumber": rn,
                    "tick": k["tick"],
                    "clutcherIndex": idx,
                    "opponentCount": len(opp_alive),
                    "won": rnd["winnerTeamKey"] == team_key,
                    "survived": idx not in dead,
                    "killCount": min(remaining_kills, 5),
                })
    return out


# ── economies ─────────────────────────────────────────────────────────────────

_ECO_ORDER = ["pistol", "eco", "semi", "force", "full"]


def _is_pistol_round(round_number: int) -> bool:
    # CS2 MR12 pistol rounds are R1 and R13; OT halves start with high money.
    return round_number in (1, 13)


def _is_pistol_conversion_round(round_number: int, team_key: str, rounds: list[dict]) -> bool:
    previous_pistol = round_number - 1
    if previous_pistol not in (1, 13):
        return False
    previous_round = next(
        (row for row in rounds if row.get("roundNumber") == previous_pistol), None)
    return previous_round is not None and previous_round.get("winnerTeamKey") == team_key


def _economy_type(money_spent: int, start_money: int, equipment_value: int,
                  round_number: int) -> str:
    if _is_pistol_round(round_number):
        return "pistol"
    if equipment_value >= 4000:
        return "full"
    if money_spent < 1000 and equipment_value < 1000:
        return "eco"
    if start_money > 0 and money_spent / start_money >= 0.80:
        return "force"
    return "semi"


def _team_economy_vote(types: list[str]) -> str:
    if not types:
        return "semi"
    counts = {t: types.count(t) for t in _ECO_ORDER}
    max_count = max(counts.values())
    for t in _ECO_ORDER:
        if counts[t] == max_count:
            return t
    return "semi"


def build_economies(raw: dict, players: PlayerDirectory, round_model: _RoundModel,
                    rounds: list[dict]) -> list[dict]:
    freeze_tick_to_round = {w.freeze_end_tick: w.round_number for w in round_model.windows}

    out = []
    team_round_types: dict[tuple[int, str], list[str]] = {}

    for r in raw.get("economy_raw", []):
        tick = int(r.get("tick") or 0)
        n = freeze_tick_to_round.get(tick, 0)
        if n <= 0:
            continue
        sid = _sid(r.get("steamid"))
        idx = players.idx(sid)
        if idx is None:
            continue
        key = players.team(sid)
        if not _is_valid_side(round_model.side_map.get((n, key), "unknown")):
            continue

        spent = int(_safe_float(r.get("cash_spent_this_round"), 0))
        equip = int(_safe_float(r.get("current_equip_value"), 0))
        start_money = int(_safe_float(r.get("start_balance"), 0))
        eco_type = _economy_type(spent, start_money, equip, n)
        primary, secondary, grenade_count, grenades = classify_inventory(r.get("inventory"))

        out.append({
            "roundNumber": n,
            "playerIndex": idx,
            "startMoney": start_money,
            "moneySpent": spent,
            "equipmentValue": equip,
            "type": eco_type,
            "hasArmor": bool(int(_safe_float(r.get("armor"), 0)) > 0),
            "hasHelmet": bool(_b(r.get("has_helmet"))),
            "hasDefuseKit": bool(_b(r.get("has_defuser"))),
            "primaryWeapon": primary,
            "secondaryWeapon": secondary,
            "grenadeCount": grenade_count,
            "grenades": grenades,
        })
        team_round_types.setdefault((n, key), []).append(eco_type)

    round_by_number = {r["roundNumber"]: r for r in rounds}
    for (rn, key), types in team_round_types.items():
        rd = round_by_number.get(rn)
        if rd is None:
            continue
        vote = _team_economy_vote(types)
        if _is_pistol_conversion_round(rn, key, rounds):
            vote = "full"
        if key == "teamA":
            rd["teamAEconomy"] = vote
        elif key == "teamB":
            rd["teamBEconomy"] = vote

    for rd in rounds:
        if rd.get("teamAEconomy") in (None, "unknown"):
            rd["teamAEconomy"] = "semi"
        if rd.get("teamBEconomy") in (None, "unknown"):
            rd["teamBEconomy"] = "semi"

    return out


# ── player-stats ──────────────────────────────────────────────────────────────

def build_player_stats(raw: dict, players: PlayerDirectory, round_model: _RoundModel,
                       rounds: list[dict], kills_list: list[dict],
                       blinds_list: list[dict], damages_list: list[dict],
                       clutches_list: list[dict]) -> list[dict]:
    total_rounds = len(rounds)
    stats: dict[int, dict] = {}

    def _get(idx: int) -> dict:
        if idx not in stats:
            stats[idx] = {
                "playerIndex": idx,
                "rounds": total_rounds,
                "kills": 0, "deaths": 0, "assists": 0,
                "damageHealth": 0, "damageArmor": 0,
                "utilityDamage": 0,
                "headshotCount": 0,
                "firstKillCount": 0, "firstDeathCount": 0,
                "tradeKillCount": 0, "tradeDeathCount": 0,
                "noScopeKillCount": 0,
                "wallbangKillCount": 0,
                "collateralKillCount": 0,
                "bombPlantCount": 0, "bombDefuseCount": 0,
                "oneKillCount": 0, "twoKillCount": 0, "threeKillCount": 0,
                "fourKillCount": 0, "fiveKillCount": 0,
                "vsOneCount": 0, "vsOneWonCount": 0, "vsOneLostCount": 0,
                "vsTwoCount": 0, "vsTwoWonCount": 0, "vsTwoLostCount": 0,
                "vsThreeCount": 0, "vsThreeWonCount": 0, "vsThreeLostCount": 0,
                "vsFourCount": 0, "vsFourWonCount": 0, "vsFourLostCount": 0,
                "vsFiveCount": 0, "vsFiveWonCount": 0, "vsFiveLostCount": 0,
                "kastRounds": 0,
                "flashAssistCount": 0,
                "enemyFlashDurationSeconds": 0.0,
                "teamFlashDurationSeconds": 0.0,
                "combatDeathCount": 0,
                "bombDeathCount": 0,
                "_rounds_with_kill": set(),
                "_rounds_with_death": set(),
                "_rounds_with_assist": set(),
                "_rounds_traded": set(),
            }
        return stats[idx]

    # Ensure every roster player has a stats row, even with zero events.
    for i in range(len(players.rows)):
        _get(i)

    # kills / deaths / assists / multi-kills from the canonical kills list
    kills_per_round: dict[int, dict[int, int]] = {}
    for k in kills_list:
        n = k["roundNumber"]
        victim = _get(k["victimIndex"])
        victim["deaths"] += 1
        victim["_rounds_with_death"].add(n)
        killer_idx = k["killerIndex"]
        if killer_idx is not None and killer_idx == k["victimIndex"]:
            killer_idx = None  # suicide
        if killer_idx is not None:
            victim["combatDeathCount"] += 1
            killer = _get(killer_idx)
            killer["kills"] += 1
            killer["_rounds_with_kill"].add(n)
            if k["headshot"]:
                killer["headshotCount"] += 1
            if k["noScope"]:
                killer["noScopeKillCount"] += 1
            if k["penetratedObjects"]:
                killer["wallbangKillCount"] += 1
            kills_per_round.setdefault(killer_idx, {})
            kills_per_round[killer_idx][n] = kills_per_round[killer_idx].get(n, 0) + 1
        else:
            victim["bombDeathCount"] += 1
        if k["tradeKill"] and k["killerIndex"] is not None:
            _get(k["killerIndex"])["tradeKillCount"] += 1
        if k["tradeDeath"]:
            victim["tradeDeathCount"] += 1
            victim["_rounds_traded"].add(n)
        if k["assisterIndex"] is not None:
            a = _get(k["assisterIndex"])
            a["assists"] += 1
            a["_rounds_with_assist"].add(n)
        if k["flashAssist"] and k["flashAssisterIndex"] is not None:
            _get(k["flashAssisterIndex"])["flashAssistCount"] += 1

    # collateral kills: 2+ enemy kills by one killer on the same tick
    collateral_groups: dict[tuple[int, int], int] = {}
    for k in kills_list:
        if k["killerIndex"] is None or k["killerIndex"] == k["victimIndex"]:
            continue
        gkey = (k["killerIndex"], k["tick"])
        collateral_groups[gkey] = collateral_groups.get(gkey, 0) + 1
    for (killer_idx, _tick), cnt in collateral_groups.items():
        if cnt >= 2:
            _get(killer_idx)["collateralKillCount"] += cnt

    # bomb plant / defuse
    for rows_key, field in (("bomb_planted", "bombPlantCount"),
                            ("bomb_defused", "bombDefuseCount")):
        for r in raw.get(rows_key, []):
            if _event_round_number(round_model, r) is None:
                continue
            idx = players.idx(_sid(r.get("user_steamid") or r.get("steamid") or r.get("userid")))
            if idx is not None:
                _get(idx)[field] += 1

    # first kill / first death per round
    first_kills: dict[int, int] = {}
    first_deaths: dict[int, int] = {}
    for k in sorted(kills_list, key=lambda x: x["tick"]):
        n = k["roundNumber"]
        if k["killerIndex"] is not None and n not in first_kills:
            first_kills[n] = k["killerIndex"]
        if n not in first_deaths:
            first_deaths[n] = k["victimIndex"]
    for idx in first_kills.values():
        _get(idx)["firstKillCount"] += 1
    for idx in first_deaths.values():
        _get(idx)["firstDeathCount"] += 1

    # multi-kill buckets
    for idx, per_round in kills_per_round.items():
        s = _get(idx)
        for count in per_round.values():
            if count == 1:
                s["oneKillCount"] += 1
            elif count == 2:
                s["twoKillCount"] += 1
            elif count == 3:
                s["threeKillCount"] += 1
            elif count == 4:
                s["fourKillCount"] += 1
            elif count >= 5:
                s["fiveKillCount"] += 1

    # damages — anti-enemy only, capped effective damage (matches damages.json)
    util_weapons = {"hegrenade", "inferno", "molotov", "incendiary"}
    for r in damages_list:
        atk = r["attackerIndex"]
        vic = r["victimIndex"]
        if atk is None or atk == vic:
            continue
        if players.team_of_index(atk) == players.team_of_index(vic):
            continue
        s = _get(atk)
        s["damageHealth"] += int(r["healthDamage"])
        s["damageArmor"] += int(r["armorDamage"])
        if str(r["weapon"] or "").lower() in util_weapons:
            s["utilityDamage"] += int(r["healthDamage"])

    # flash durations
    for blind in blinds_list:
        flasher = blind["flasherIndex"]
        flashed = blind["flashedIndex"]
        dur = float(blind["durationSeconds"] or 0)
        if players.team_of_index(flasher) != players.team_of_index(flashed):
            _get(flasher)["enemyFlashDurationSeconds"] += dur
        else:
            _get(flasher)["teamFlashDurationSeconds"] += dur

    # KAST: kill / assist / survived / traded
    all_rounds = {r["roundNumber"] for r in rounds}
    for s in stats.values():
        survived = all_rounds - s["_rounds_with_death"]
        kast = (s["_rounds_with_kill"] | s["_rounds_with_assist"]
                | survived | s["_rounds_traded"])
        s["kastRounds"] = len(kast & all_rounds)

    # clutch buckets
    for c in clutches_list:
        s = _get(c["clutcherIndex"])
        prefix = ["", "vsOne", "vsTwo", "vsThree", "vsFour", "vsFive"][min(c["opponentCount"], 5)]
        s[f"{prefix}Count"] += 1
        s[f"{prefix}WonCount" if c["won"] else f"{prefix}LostCount"] += 1

    out = []
    for idx in sorted(stats.keys()):
        s = stats[idx]
        row = {k: v for k, v in s.items() if not k.startswith("_")}
        row["adr"] = round(s["damageHealth"] / max(total_rounds, 1), 2)
        row["kast"] = round(s["kastRounds"] / max(total_rounds, 1) * 100, 3)
        row["averageUtilityDamagePerRound"] = round(s["utilityDamage"] / max(total_rounds, 1), 2)
        row["enemyFlashDurationSeconds"] = round(row["enemyFlashDurationSeconds"], 3)
        row["teamFlashDurationSeconds"] = round(row["teamFlashDurationSeconds"], 3)
        out.append(row)
    return out
