"""demoparser2 extraction for the v3 exporter.

Performance notes (this module is the reference for fast producers):

- Event scans are batched: one `parse_events` call per group of events that
  share extra props, instead of one demo scan per event.
- The big per-frame `parse_ticks` result is kept as a pandas DataFrame all the
  way into the columnar stream builders. v2-era exporters converted it to
  ~200k row dicts (`to_dict(orient="records")`), which dominated Python-side
  time; v3's columnar layout makes that conversion unnecessary.
- `inventory` (a per-row list of strings, by far the most expensive tick prop)
  is extracted on the sampled replay grid for held utility and bomb-carrier
  state, and at freeze ticks for economy rows.
- The research-profile duel windows reuse the already-parsed kill/damage ticks
  to compute merged combat windows, then fetch them in ONE lean `parse_ticks`
  call (6 props at full tick) instead of widening the main grid.

Provenance: event extraction layout ported from cs2-demo-analysis-kit
(originally DrEAmSs59/CS2-insight-agent, with the author's permission).
"""

from __future__ import annotations

import logging
import time
from typing import TYPE_CHECKING, Any, Callable

if TYPE_CHECKING:
    import pandas as pd
    from demoparser2 import DemoParser  # type: ignore

log = logging.getLogger(__name__)

ProgressFn = Callable[[str, float], None]

# 火用 inferno_startburn(起火)作为 effect 事件;inferno_expire(熄灭)单独
# 解析,由 events.py 配对成 destroyTick。
_GRENADE_EVENTS = [
    ("smokegrenade_detonate", "smoke"),
    ("flashbang_detonate", "flashbang"),
    ("hegrenade_detonate", "hegrenade"),
    ("inferno_startburn", "molotov"),
    ("decoy_detonate", "decoy"),
]

# steamid/XYZ for grenade detonations — resolve thrower via player extras
# (raw userid is an entity slot, not a Steam64).
_GRENADE_PLAYER_FIELDS = ["steamid", "X", "Y", "Z"]

# Per-frame props for the replay grid. `inventory` is sampled here so replay
# consumers can render current held utility; full-tick paths still avoid it.
# `balance` is the cash account (replay `money` column);
# `current_equip_value` is the equipment value (`equipValue` column).
_REPLAY_PROPS = [
    "steamid", "team_num", "X", "Y", "Z", "yaw", "pitch",
    "health", "armor", "active_weapon_name", "flash_duration",
    "balance", "current_equip_value", "has_defuser", "last_place_name", "inventory",
]

# Lean per-frame props for full-tick duel windows.
_DUEL_PROPS = ["steamid", "X", "Y", "Z", "yaw", "pitch", "health", "flash_duration"]


def _rows(result: Any) -> list[dict]:
    """Convert a demoparser2 result (DataFrame or list) to a list of dicts.

    Only used for small event tables; per-frame data stays as DataFrames.
    """
    if result is None:
        return []
    if hasattr(result, "to_dict"):
        return result.to_dict(orient="records")
    return list(result)


def _raise_if_control_flow(exc: BaseException) -> None:
    if isinstance(exc, (KeyboardInterrupt, SystemExit)):
        raise exc


def _safe_event(parser: DemoParser, event: str,
                other: list[str] | None = None,
                player: list[str] | None = None) -> list[dict]:
    try:
        kwargs: dict[str, list[str]] = {}
        if other is not None:
            kwargs["other"] = other
        if player is not None:
            kwargs["player"] = player
        return _rows(parser.parse_event(event, **kwargs))
    except Exception:
        return []


def _safe_events(parser: DemoParser, names: list[str],
                 other: list[str] | None = None,
                 player: list[str] | None = None) -> dict[str, list[dict]]:
    """Batch-parse several events in ONE demo scan via parse_events."""
    kwargs: dict[str, list[str]] = {}
    if other is not None:
        kwargs["other"] = other
    if player is not None:
        kwargs["player"] = player
    try:
        pairs = parser.parse_events(names, **kwargs)
        out = {name: _rows(df) for name, df in pairs}
    except Exception:
        log.warning("parse_events(%s) failed; falling back to per-event parse", names, exc_info=True)
        out = {name: _safe_event(parser, name, other=other, player=player) for name in names}
    for name in names:
        out.setdefault(name, [])
    return out


def _safe_ticks_df(parser: DemoParser, props: list[str], ticks: list[int]) -> "pd.DataFrame | None":
    """parse_ticks kept as a DataFrame; drops unknown props one by one on failure."""
    remaining = list(props)
    while remaining:
        try:
            return parser.parse_ticks(remaining, ticks=ticks)
        except Exception as exc:
            msg = str(exc)
            dropped = next((p for p in remaining if p in msg), None)
            if dropped is None:
                log.warning("parse_ticks failed: %s", msg)
                return None
            log.warning("parse_ticks: dropping unsupported prop %r", dropped)
            remaining.remove(dropped)
    return None


def _rebuild_blinds(parser: DemoParser, detonates: list[dict]) -> list[dict]:
    """Reconstruct per-victim blind rows from flashbang_detonate + flash_duration.

    Newer demoparser2 builds no longer emit the player_blind game event; the only
    flash signal left is flashbang_detonate (thrower + position) plus the
    per-player flash_duration tick property. A player whose flash_duration jumps
    across a detonate tick was blinded by that flash; the post-detonate value is
    the (remaining ≈ full) blind duration. When several flashes pop on the same
    tick, attribute by closest detonate position.
    """
    det_by_tick: dict[int, list[dict]] = {}
    for d in detonates:
        tick = int(d.get("tick") or 0)
        if tick > 0:
            det_by_tick.setdefault(tick, []).append(d)
    if not det_by_tick:
        return []
    det_ticks = sorted(det_by_tick)
    sample_ticks = sorted({t - 1 for t in det_ticks} | {t + 2 for t in det_ticks})
    df = _safe_ticks_df(parser, ["flash_duration", "X", "Y"], sample_ticks)
    if df is None or df.empty or "flash_duration" not in df.columns:
        return []

    state: dict[tuple[int, str], tuple[float, float | None, float | None]] = {}
    has_xy = "X" in df.columns and "Y" in df.columns
    for row in df.itertuples(index=False):
        d = row._asdict()
        sid = d.get("steamid")
        if sid is None:
            continue
        fd = float(d.get("flash_duration") or 0.0)
        x = float(d["X"]) if has_xy and d.get("X") is not None else None
        y = float(d["Y"]) if has_xy and d.get("Y") is not None else None
        state[(int(d["tick"]), str(int(sid)))] = (fd, x, y)

    out: list[dict] = []
    sids = {sid for (_, sid) in state}
    for tick in det_ticks:
        dets = det_by_tick[tick]
        for sid in sids:
            before = state.get((tick - 1, sid), (0.0, None, None))[0]
            after_state = state.get((tick + 2, sid))
            if after_state is None:
                continue
            after, px, py = after_state
            if after <= before + 0.1:
                continue
            det = dets[0]
            if len(dets) > 1 and px is not None and py is not None:
                det = min(
                    dets,
                    key=lambda d: (float(d.get("x") or 0.0) - px) ** 2 + (float(d.get("y") or 0.0) - py) ** 2,
                )
            out.append({
                "tick": tick,
                "attacker_steamid": det.get("user_steamid"),
                "user_steamid": sid,
                "blind_duration": after,
            })
    return out


def parse_demo(dem_path: str, *, sample_rate: int = 8, research: bool = False,
               window_before_ms: int = 2000, window_after_ms: int = 1000,
               progress: ProgressFn | None = None) -> dict[str, Any]:
    """Full extraction. Event tables are lists of dicts; per-frame data is DataFrames."""
    from demoparser2 import DemoParser  # lazy: native dep

    timings: dict[str, float] = {}
    parse_started = time.perf_counter()

    def _record(stage: str, started: float) -> None:
        timings[stage] = round(time.perf_counter() - started, 3)

    def _p(stage: str, frac: float) -> None:
        if progress is not None:
            progress(stage, frac)

    _p("open demo", 0.01)
    stage_started = time.perf_counter()
    p = DemoParser(dem_path)
    _record("parse.openDemo", stage_started)

    stage_started = time.perf_counter()
    try:
        header = dict(p.parse_header())
    except BaseException as exc:
        _raise_if_control_flow(exc)
        header = {}
    _record("parse.header", stage_started)
    try:
        tickrate = int(float(header.get("tick_rate") or 64))
    except (TypeError, ValueError):
        tickrate = 64

    _p("round events", 0.05)
    stage_started = time.perf_counter()
    g_round = _safe_events(p,
        ["round_start", "round_freeze_end", "round_end", "player_blind",
         "round_announce_match_start"],
        other=["winner", "reason", "legacy", "blind_duration", "total_rounds_played"],
    )
    _record("parse.roundEvents", stage_started)

    # demoparser2 新版已移除 player_blind 事件：用 flashbang_detonate + flash_duration
    # tick 采样重建逐人致盲数据（旧版仍发 player_blind 时维持原路径）。
    if not g_round["player_blind"]:
        stage_started = time.perf_counter()
        detonates = _safe_event(p, "flashbang_detonate")
        g_round["player_blind"] = _rebuild_blinds(p, detonates)
        _record("parse.blindRebuild", stage_started)

    _p("kills", 0.12)
    stage_started = time.perf_counter()
    deaths = _safe_event(p, "player_death",
        other=["headshot", "noscope", "thrusmoke", "penetrated", "penetrated_objects",
               "assistedflash", "attackerblind", "total_rounds_played"],
        player=["X", "Y", "Z", "active_weapon"],
    )
    _record("parse.kills", stage_started)

    _p("damages", 0.2)
    stage_started = time.perf_counter()
    hurts = _safe_event(p, "player_hurt",
        other=["weapon", "hitgroup", "dmg_health", "dmg_armor", "health", "armor",
               "total_rounds_played"],
        player=["X", "Y", "Z"],
    )
    _record("parse.damages", stage_started)

    _p("shots", 0.28)
    stage_started = time.perf_counter()
    fires = _safe_event(p, "weapon_fire",
        other=["weapon", "total_rounds_played"],
        player=["X", "Y", "Z", "yaw", "pitch"],
    )
    _record("parse.shotsEvents", stage_started)
    # velocity is not available via weapon_fire player extras; fetch from tick data.
    stage_started = time.perf_counter()
    fire_velocity_df = None
    if fires:
        shot_ticks = sorted({int(r["tick"]) for r in fires if int(r.get("tick") or 0) > 0})
        if shot_ticks:
            fire_velocity_df = _safe_ticks_df(
                p, ["steamid", "velocity_X", "velocity_Y", "velocity_Z"], shot_ticks)
    _record("parse.shotsVelocity", stage_started)

    _p("bomb events", 0.36)
    stage_started = time.perf_counter()
    g_bomb = _safe_events(p,
        ["bomb_planted", "bomb_defused", "bomb_exploded",
         "bomb_beginplant", "bomb_begindefuse", "bomb_dropped", "bomb_pickup"],
        other=["site", "total_rounds_played"],
        player=["steamid", "X", "Y", "Z", "last_place_name"])
    _record("parse.bombEvents", stage_started)

    _p("grenade detonations", 0.42)
    stage_started = time.perf_counter()
    g_nade = _safe_events(p,
        [name for name, _ in _GRENADE_EVENTS] + ["inferno_expire", "smokegrenade_expired"],
        other=["total_rounds_played"], player=_GRENADE_PLAYER_FIELDS)
    grenade_detonations: list[dict] = []
    for ev_name, gtype in _GRENADE_EVENTS:
        grenade_detonations.extend({**r, "_grenade_type": gtype} for r in g_nade[ev_name])
    _record("parse.grenadeEvents", stage_started)

    # ── player info / team names at match start ──────────────────
    stage_started = time.perf_counter()
    announce_rows = g_round["round_announce_match_start"]
    round_freeze_ends = g_round["round_freeze_end"]
    if announce_rows:
        match_start_tick = int(announce_rows[0]["tick"])
    elif round_freeze_ends:
        match_start_tick = int(round_freeze_ends[0]["tick"])
    else:
        match_start_tick = 1

    team_a_name: str | None = None
    team_b_name: str | None = None
    try:
        for row in _rows(p.parse_ticks(
                ["CCSTeam.m_szClanTeamname", "CCSTeam.m_iTeamNum"],
                ticks=[match_start_tick])):
            tn = row.get("CCSTeam.m_iTeamNum")
            name = str(row.get("CCSTeam.m_szClanTeamname") or "").strip()
            if not name or name.lower() in ("ct", "terrorist", "t", "team a", "team b"):
                continue
            if tn == 2:
                team_a_name = name
            elif tn == 3:
                team_b_name = name
    except BaseException as exc:
        _raise_if_control_flow(exc)
        pass

    try:
        player_info = _rows(p.parse_ticks(
            ["name", "steamid", "team_num", "team_name"], ticks=[match_start_tick]))
    except BaseException as exc:
        _raise_if_control_flow(exc)
        player_info = []

    # Side ground truth sampled shortly after each freeze ends.
    round_side_ticks = sorted({
        int(r["tick"]) + 16 for r in round_freeze_ends if int(r.get("tick") or 0) > 0
    })
    round_side_samples: list[dict] = []
    if round_side_ticks:
        try:
            round_side_samples = _rows(p.parse_ticks(["steamid", "team_num"],
                                                     ticks=round_side_ticks))
        except BaseException as exc:
            _raise_if_control_flow(exc)
            round_side_samples = []
    _record("parse.playerInfo", stage_started)

    # ── replay grid (single DataFrame, no row dicts) ─────────────
    _p("replay grid (slowest stage)", 0.5)
    stage_started = time.perf_counter()
    round_ends = g_round["round_end"]
    step = max(1, tickrate // max(1, sample_rate))
    replay_ticks = _build_sample_ticks(round_ends, round_freeze_ends, g_round["round_start"], step)
    replay_df = _safe_ticks_df(p, _REPLAY_PROPS, replay_ticks) if replay_ticks else None
    _record("parse.replayGrid", stage_started)

    _p("grenade trajectories", 0.72)
    stage_started = time.perf_counter()
    grenade_throws, grenade_trajectories = _extract_grenade_paths(p, replay_ticks)
    _record("parse.grenadeTrajectories", stage_started)

    # ── duel windows (research profile): full-tick lean parse ────
    stage_started = time.perf_counter()
    duel_df = None
    duel_windows: list[tuple[int, int]] = []
    if research:
        _p("duel windows (full tick)", 0.78)
        anchor_ticks = [int(r.get("tick") or 0) for r in deaths + hurts]
        duel_windows = _merge_windows(anchor_ticks, round_ends, round_freeze_ends,
                                      g_round["round_start"], tickrate,
                                      window_before_ms, window_after_ms)
        duel_ticks: list[int] = []
        for start, end in duel_windows:
            duel_ticks.extend(range(start, end + 1))
        if duel_ticks:
            duel_df = _safe_ticks_df(p, _DUEL_PROPS, duel_ticks)
    _record("parse.duelWindows", stage_started)

    _p("economy", 0.92)
    stage_started = time.perf_counter()
    freeze_ticks = sorted({int(r["tick"]) for r in round_freeze_ends if r.get("tick")})
    economy_raw: list[dict] = []
    if freeze_ticks:
        try:
            economy_raw = _rows(p.parse_ticks(
                ["steamid", "team_num", "cash_spent_this_round", "current_equip_value",
                 "start_balance", "armor", "has_helmet", "has_defuser", "inventory"],
                ticks=freeze_ticks))
        except BaseException as exc:
            _raise_if_control_flow(exc)
            economy_raw = []
    _record("parse.economy", stage_started)
    timings["parse.total"] = round(time.perf_counter() - parse_started, 3)

    return {
        "_timings": timings,
        "header": header,
        "tickrate": tickrate,
        "sample_rate": max(1, tickrate // step),
        "match_start_tick": match_start_tick,
        "team_a_name": team_a_name,
        "team_b_name": team_b_name,
        "player_info": player_info,
        "round_starts": g_round["round_start"],
        "round_freeze_ends": round_freeze_ends,
        "round_ends": round_ends,
        "deaths": deaths,
        "hurts": hurts,
        "fires": fires,
        "fire_velocity_df": fire_velocity_df,
        "blinds": g_round["player_blind"],
        "bomb_planted": g_bomb["bomb_planted"],
        "bomb_defused": g_bomb["bomb_defused"],
        "bomb_exploded": g_bomb["bomb_exploded"],
        "bomb_beginplant": g_bomb["bomb_beginplant"],
        "bomb_begindefuse": g_bomb["bomb_begindefuse"],
        "bomb_dropped": g_bomb["bomb_dropped"],
        "bomb_pickup": g_bomb["bomb_pickup"],
        "grenade_detonations": grenade_detonations,
        "inferno_expires": g_nade["inferno_expire"],
        "smoke_expires": g_nade["smokegrenade_expired"],
        "grenade_throws": grenade_throws,
        "grenade_trajectories": grenade_trajectories,
        "replay_df": replay_df,
        "replay_ticks": replay_ticks,
        "duel_df": duel_df,
        "duel_windows": duel_windows,
        "round_side_samples": round_side_samples,
        "economy_raw": economy_raw,
        "freeze_ticks": freeze_ticks,
    }


# ── sampling grids ────────────────────────────────────────────────────────────

def _freeze_by_round(round_freeze_ends: list[dict]) -> dict[int, int]:
    """total_rounds_played at round_freeze_end = N-1 for round N."""
    out: dict[int, int] = {}
    for r in round_freeze_ends:
        rn = int(r.get("total_rounds_played") or 0) + 1
        t = int(r.get("tick") or 0)
        if rn > 0 and t > 0:
            out[rn] = t
    return out


def _round_starts_by_round(round_starts: list[dict]) -> dict[int, int]:
    out: dict[int, int] = {}
    for r in round_starts:
        rn = int(r.get("total_rounds_played") or 0) + 1
        t = int(r.get("tick") or 0)
        if rn > 0 and t > 0 and rn not in out:
            out[rn] = t
    return out


def _round_spans(round_ends: list[dict],
                 round_freeze_ends: list[dict],
                 round_starts: list[dict] | None = None) -> list[tuple[int, int]]:
    """[(freeze_end_tick, event_end_tick)] for rounds with a valid window."""
    freeze = _freeze_by_round(round_freeze_ends)
    starts = _round_starts_by_round(round_starts or [])
    spans: list[tuple[int, int]] = []
    for r in round_ends:
        rn = int(r.get("total_rounds_played") or 0)
        end_t = int(r.get("tick") or 0)
        next_start_t = starts.get(rn + 1)
        event_end_t = next_start_t - 1 if next_start_t is not None else end_t
        start_t = freeze.get(rn, 0)
        if start_t > 0 and event_end_t > start_t:
            spans.append((start_t, event_end_t))
    return spans


def _build_sample_ticks(round_ends: list[dict], round_freeze_ends: list[dict],
                        round_starts: list[dict],
                        step: int) -> list[int]:
    """Sorted unique sample ticks through the post-round tail."""
    ticks: list[int] = []
    for start_t, end_t in _round_spans(round_ends, round_freeze_ends, round_starts):
        ticks.extend(range(start_t, end_t + 1, step))
    return sorted(set(ticks))


def _merge_windows(anchor_ticks: list[int], round_ends: list[dict],
                   round_freeze_ends: list[dict], round_starts: list[dict],
                   tickrate: int,
                   before_ms: int, after_ms: int) -> list[tuple[int, int]]:
    """Merged [start, end] full-tick combat windows, clamped to round spans."""
    spans = _round_spans(round_ends, round_freeze_ends, round_starts)
    if not spans or not anchor_ticks:
        return []
    before = (before_ms * tickrate) // 1000
    after = (after_ms * tickrate) // 1000
    windows: list[tuple[int, int]] = []
    spans.sort()
    anchors = sorted(set(t for t in anchor_ticks if t > 0))
    import bisect
    starts = [s for s, _ in spans]
    for t in anchors:
        i = bisect.bisect_right(starts, t) - 1
        if i < 0:
            continue
        lo, hi = spans[i]
        if not (lo <= t <= hi):
            continue
        windows.append((max(lo, t - before), min(hi, t + after)))
    if not windows:
        return []
    windows.sort()
    merged = [windows[0]]
    for s, e in windows[1:]:
        ls, le = merged[-1]
        if s <= le + 1:
            merged[-1] = (ls, max(le, e))
        else:
            merged.append((s, e))
    return merged


# ── grenade trajectories ──────────────────────────────────────────────────────

def _nearest_path(path: dict[int, tuple], t: int) -> tuple:
    if not path:
        return (0.0, 0.0, 0.0)
    k = min(path.keys(), key=lambda kt: abs(kt - t))
    return path[k]


def _extract_grenade_paths(parser: DemoParser,
                           sample_ticks: list[int]) -> tuple[list[dict], list[dict]]:
    """Throw origins + in-flight trajectories from parse_grenades().

    trajectories are sampled onto the replay grid for replay rendering:
    {grenade, steamid, start_tick, xs, ys, zs}. Flight phase only; the static
    smoke/fire effect afterwards lives in grenades.json.
    """
    from .enums import grenade_projectile_to_type

    try:
        g = parser.parse_grenades()
    except Exception:
        return [], []
    if g is None or not hasattr(g, "columns") or "grenade_entity_id" not in g.columns:
        return [], []
    try:
        proj = g[g["grenade_type"].astype(str).str.endswith("Projectile")]
        proj = proj.dropna(subset=["x", "y", "z"]).sort_values(["grenade_entity_id", "tick"])
        if proj.empty:
            return [], []
        # Entity ids recycle; a new throw starts when the id changes or the
        # per-tick flight path breaks (gap > ~1s).
        eid = proj["grenade_entity_id"]
        tick = proj["tick"]
        seg = ((eid != eid.shift()) | ((tick - tick.shift()) > 64)).cumsum()
        proj = proj.assign(_seg=seg)
        grouped = proj.groupby("_seg", sort=False)
        first = grouped.first()
        last_tick = grouped["tick"].last()
    except Exception:
        return [], []

    grid = sorted({int(t) for t in (sample_ticks or [])})

    throws: list[dict] = []
    trajectories: list[dict] = []
    for seg_id, seg_rows in grouped:
        row = first.loc[seg_id]
        gtype = grenade_projectile_to_type(row.get("grenade_type"))
        if gtype is None:
            continue
        eid_val = row.get("grenade_entity_id")
        throw_tick = int(row["tick"])
        last = int(last_tick.loc[seg_id])
        steamid = row.get("steamid")
        throws.append({
            "grenade_entity_id": int(eid_val) if eid_val is not None else None,
            "grenade": gtype,
            "tick": throw_tick,
            "destroy_tick": last,
            "steamid": steamid,
            "X": float(row["x"]),
            "Y": float(row["y"]),
            "Z": float(row["z"]),
        })

        path = {
            int(t): (x, y, z)
            for t, x, y, z in zip(seg_rows["tick"], seg_rows["x"], seg_rows["y"], seg_rows["z"])
        }
        gticks = [t for t in grid if throw_tick <= t <= last]
        if not gticks:
            gticks = [throw_tick]
            path.setdefault(throw_tick, (row["x"], row["y"], row["z"]))
        xs: list[int] = []
        ys: list[int] = []
        zs: list[int] = []
        for t in gticks:
            pos = path.get(t) or _nearest_path(path, t)
            xs.append(int(round(pos[0])))
            ys.append(int(round(pos[1])))
            zs.append(int(round(pos[2])))
        # Trim the stationary at-rest tail (smoke/decoy entities linger at rest).
        while len(xs) >= 2 and (
            (xs[-1] - xs[-2]) ** 2 + (ys[-1] - ys[-2]) ** 2 + (zs[-1] - zs[-2]) ** 2
        ) <= 100:
            xs.pop()
            ys.pop()
            zs.pop()
        trajectories.append({
            "grenade": gtype,
            "steamid": steamid,
            "start_tick": gticks[0],
            "xs": xs,
            "ys": ys,
            "zs": zs,
        })

    return throws, trajectories
