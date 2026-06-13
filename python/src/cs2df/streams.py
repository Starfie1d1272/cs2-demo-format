"""Columnar stream builders: replay.json, duels.json, shots.json.

This is the performance-critical path. The per-frame DataFrame from
demoparser2 is processed entirely with pandas/numpy — grouped, reindexed onto
the round grid, delta-encoded with np.diff — and only materialized as Python
lists at JSON-serialization time. No per-row dict conversion ever happens.

Delta convention (see schemas/index.ts): stored[0] = v[0],
stored[i] = v[i] − v[i−1]; implemented as np.diff(v, prepend=0).
"""

from __future__ import annotations

from typing import Any

from .enums import classify_inventory, normalize_weapon_name
from .events import PlayerDirectory, _sid
from .rounds import _RoundModel

ANGLE_SCALE = 10  # stored angle = degrees * 10 (0.1° resolution)
COORD_SCALE = 1   # game units, rounded to int
FLAG_ALIVE = 1
FLAG_HAS_BOMB = 2
FLAG_HAS_DEFUSE_KIT = 4


class _Dict:
    """Insertion-ordered string dictionary → index, -1 for None."""

    def __init__(self) -> None:
        self.values: list[str] = []
        self._idx: dict[str, int] = {}

    def index(self, value: str | None) -> int:
        if value is None or value == "":
            return -1
        i = self._idx.get(value)
        if i is None:
            i = len(self.values)
            self._idx[value] = i
            self.values.append(value)
        return i


def _delta(arr) -> list[int]:
    import numpy as np

    a = np.asarray(arr, dtype="int64")
    return np.diff(a, prepend=np.int64(0)).tolist()


def _prep_frame_df(df, players: PlayerDirectory):
    """Common normalization: sid → playerIndex column, sorted by tick."""
    import numpy as np
    import pandas as pd

    if df is None or len(df) == 0:
        return None
    df = df.copy()
    sid = pd.to_numeric(df["steamid"], errors="coerce").fillna(0).astype("int64").astype(str)
    pidx = sid.map(players.index_by_sid)
    df["_pidx"] = pidx
    df = df[df["_pidx"].notna()]
    if len(df) == 0:
        return None
    df["_pidx"] = df["_pidx"].astype("int64")
    df["tick"] = pd.to_numeric(df["tick"], errors="coerce").fillna(0).astype("int64")
    return df.sort_values("tick", kind="stable")


def _num(series, fill=0.0):
    import pandas as pd

    return pd.to_numeric(series, errors="coerce").fillna(fill)


def _slice_by_tick(df, tick_values, start: int, end: int):
    """Return rows with start <= tick <= end from a tick-sorted DataFrame."""
    import numpy as np

    left = int(np.searchsorted(tick_values, start, side="left"))
    right = int(np.searchsorted(tick_values, end, side="right"))
    if right <= left:
        return df.iloc[0:0]
    return df.iloc[left:right]


# ── bomb carrier timeline (replaces per-frame inventory parsing) ──────────────

def build_bomb_carrier_timeline(raw: dict, players: PlayerDirectory):
    """(transition_ticks, carrier_pidx) arrays for hasBomb lookups.

    Derived from bomb lifecycle events: pickup → carrier, dropped/planted/
    exploded/defused → no carrier. This replaces extracting the `inventory`
    tick prop on the whole replay grid, which is the single most expensive
    per-frame property in demoparser2.
    """
    import numpy as np

    transitions: list[tuple[int, int]] = []  # (tick, pidx or -1)
    for rows_key, is_pickup in (("bomb_pickup", True), ("bomb_dropped", False),
                                ("bomb_planted", False), ("bomb_exploded", False),
                                ("bomb_defused", False)):
        for r in raw.get(rows_key, []):
            tick = int(r.get("tick") or 0)
            if tick <= 0:
                continue
            if is_pickup:
                idx = players.idx(_sid(r.get("user_steamid") or r.get("steamid")))
                transitions.append((tick, idx if idx is not None else -1))
            else:
                transitions.append((tick, -1))
    transitions.sort()
    ticks = np.asarray([t for t, _ in transitions], dtype="int64")
    carrier = np.asarray([c for _, c in transitions], dtype="int64")
    return ticks, carrier


def _carrier_at(ticks, carrier, frame_ticks):
    """Vectorized carrier playerIndex (or -1) at each frame tick."""
    import numpy as np

    if len(ticks) == 0:
        return np.full(len(frame_ticks), -1, dtype="int64")
    pos = np.searchsorted(ticks, frame_ticks, side="right") - 1
    out = np.where(pos >= 0, carrier[np.clip(pos, 0, None)], -1)
    return out


# ── replay.json ───────────────────────────────────────────────────────────────

def build_replay(raw: dict, players: PlayerDirectory, round_model: _RoundModel,
                 tickrate: int, sample_rate: int) -> dict | None:
    import numpy as np

    df = _prep_frame_df(raw.get("replay_df"), players)
    if df is None:
        return None

    step = max(1, tickrate // max(1, sample_rate))
    weapon_dict = _Dict()
    place_dict = _Dict()

    # Pre-map dictionary columns on UNIQUE values only (cheap), then map rows.
    weapon_map = {
        v: weapon_dict.index(normalize_weapon_name(v))
        for v in df["active_weapon_name"].dropna().unique()
    }
    df["_widx"] = df["active_weapon_name"].map(weapon_map).fillna(-1).astype("int64")
    place_map = {
        v: place_dict.index(str(v))
        for v in df["last_place_name"].dropna().unique()
        if str(v).strip()
    }
    df["_plidx"] = df["last_place_name"].map(place_map).fillna(-1).astype("int64")
    if "inventory" in df.columns:
        df["_grenades"] = df["inventory"].map(lambda items: classify_inventory(items)[3])
    else:
        df["_grenades"] = [[] for _ in range(len(df))]

    bomb_ticks, bomb_carrier = build_bomb_carrier_timeline(raw, players)
    tick_values = df["tick"].to_numpy()

    # projectile trajectories per round
    proj_by_round: dict[int, list[dict]] = {}
    for tr in raw.get("grenade_trajectories", []):
        start = int(tr.get("start_tick") or 0)
        rn = round_model.round_for_tick(start)
        if rn is None:
            continue
        proj_by_round.setdefault(rn, []).append({
            "grenade": tr["grenade"],
            "throwerIndex": players.idx(_sid(tr.get("steamid"))),
            "startTick": start,
            "x": _delta([int(v) for v in tr["xs"]]),
            "y": _delta([int(v) for v in tr["ys"]]),
            "z": _delta([int(v) for v in tr["zs"]]),
        })

    rounds_out: list[dict] = []
    windows = sorted(round_model.windows, key=lambda w: w.round_number)
    for i, w in enumerate(windows):
        next_start_tick = windows[i + 1].start_tick if i + 1 < len(windows) else None
        stop_tick = next_start_tick if next_start_tick is not None else w.end_tick + 1
        grid = np.arange(w.freeze_end_tick, stop_tick, step, dtype="int64")
        if len(grid) == 0:
            continue
        sl = _slice_by_tick(df, tick_values, int(grid[0]), int(grid[-1]))
        if len(sl) == 0:
            continue
        player_tracks = []
        for pidx, g in sl.groupby("_pidx", sort=True):
            track = _player_track(g, grid, int(pidx), bomb_ticks, bomb_carrier)
            if track is not None:
                player_tracks.append(track)
        if not player_tracks:
            continue
        rounds_out.append({
            "roundNumber": w.round_number,
            "startTick": int(grid[0]),
            "tickStep": step,
            "frameCount": int(len(grid)),
            "players": player_tracks,
            "projectiles": proj_by_round.get(w.round_number, []),
        })

    if not rounds_out:
        return None
    return {
        "meta": {
            "sampleRate": max(1, tickrate // step),
            "tickrate": tickrate,
            "coordScale": COORD_SCALE,
            "angleScale": ANGLE_SCALE,
        },
        "weaponDict": weapon_dict.values,
        "placeDict": place_dict.values,
        "rounds": rounds_out,
    }


def _player_track(g, grid, pidx: int, bomb_ticks, bomb_carrier) -> dict | None:
    """One player's columnar track aligned to the round grid.

    Frames where the player has no row (dead/disconnected) repeat the last
    live value for positional columns (keeps deltas at zero) and carry
    hp=0 / flags=0 / weapon=-1.
    """
    import numpy as np

    g = g.drop_duplicates(subset="tick", keep="last").set_index("tick")
    aligned = g.reindex(grid)
    present = aligned["_pidx"].notna().to_numpy()
    if not present.any():
        return None

    def col_ffill(name, scale=1.0):
        s = _num(aligned[name].ffill().bfill(), 0.0) * scale
        return np.round(s.to_numpy()).astype("int64")

    x = col_ffill("X")
    y = col_ffill("Y")
    z = col_ffill("Z")
    yaw = col_ffill("yaw", ANGLE_SCALE)
    pitch = col_ffill("pitch", ANGLE_SCALE)
    money = col_ffill("balance") if "balance" in aligned.columns else np.zeros(len(grid), dtype="int64")
    equip = col_ffill("current_equip_value")

    hp = np.round(_num(aligned["health"], 0.0).to_numpy()).astype("int64").clip(0, 100)
    hp = np.where(present, hp, 0)
    armor = np.round(_num(aligned["armor"], 0.0).to_numpy()).astype("int64").clip(0, 100)
    armor = np.where(present, armor, 0)
    flash = np.round(_num(aligned["flash_duration"], 0.0).to_numpy() * 10).astype("int64").clip(0, 60)
    flash = np.where(present, flash, 0)
    widx = aligned["_widx"].fillna(-1).astype("int64").to_numpy()
    widx = np.where(present, widx, -1)
    plidx = aligned["_plidx"].fillna(-1).astype("int64").to_numpy()
    plidx = np.where(present, plidx, -1)
    grenades = _align_grenades(aligned, present)

    alive = (hp > 0).astype("int64")
    has_kit = _num(aligned["has_defuser"], 0.0).to_numpy().astype(bool)
    carrier = _carrier_at(bomb_ticks, bomb_carrier, grid)
    has_bomb = (carrier == pidx) & (alive == 1)
    flags = (alive * FLAG_ALIVE
             + has_bomb.astype("int64") * FLAG_HAS_BOMB
             + (has_kit & present & (alive == 1)).astype("int64") * FLAG_HAS_DEFUSE_KIT)

    return {
        "playerIndex": pidx,
        "x": _delta(x), "y": _delta(y), "z": _delta(z),
        "yaw": _delta(yaw), "pitch": _delta(pitch),
        "hp": hp.tolist(),
        "armor": armor.tolist(),
        "money": _delta(money),
        "equipValue": _delta(equip),
        "weapon": widx.tolist(),
        "place": plidx.tolist(),
        "flash": flash.tolist(),
        "flags": flags.tolist(),
        "grenades": grenades,
    }


def _align_grenades(aligned, present) -> list[list[str]]:
    """Forward-fill sampled inventory lists and clear them when no player row exists."""
    if "_grenades" not in aligned.columns:
        return [[] for _ in range(len(aligned))]
    filled = aligned["_grenades"].ffill().bfill()
    out: list[list[str]] = []
    for has_row, value in zip(present, filled, strict=False):
        if not has_row or not isinstance(value, list):
            out.append([])
        else:
            out.append([str(item) for item in value])
    return out


# ── duels.json ────────────────────────────────────────────────────────────────

def build_duels(raw: dict, players: PlayerDirectory, round_model: _RoundModel,
                tickrate: int, kills_list: list[dict], damages_list: list[dict],
                window_before_ms: int, window_after_ms: int) -> dict | None:
    import numpy as np

    df = _prep_frame_df(raw.get("duel_df"), players)
    windows: list[tuple[int, int]] = raw.get("duel_windows") or []
    if df is None or not windows:
        return None

    # anchors grouped per merged window
    anchors_all: list[dict] = []
    for k in kills_list:
        anchors_all.append({"kind": "kill", "tick": k["tick"],
                            "attackerIndex": k["killerIndex"],
                            "victimIndex": k["victimIndex"]})
    for d in damages_list:
        anchors_all.append({"kind": "damage", "tick": d["tick"],
                            "attackerIndex": d["attackerIndex"],
                            "victimIndex": d["victimIndex"]})
    anchors_all.sort(key=lambda a: a["tick"])
    tick_values = df["tick"].to_numpy()

    out_windows: list[dict] = []
    for start, end in windows:
        rn = round_model.round_for_tick(start)
        if rn is None:
            continue
        grid = np.arange(start, end + 1, dtype="int64")
        sl = _slice_by_tick(df, tick_values, int(start), int(end))
        if len(sl) == 0:
            continue
        anchors = [a for a in anchors_all if start <= a["tick"] <= end]
        if not anchors:
            continue
        tracks = []
        for pidx, g in sl.groupby("_pidx", sort=True):
            track = _duel_track(g, grid, int(pidx))
            if track is not None:
                tracks.append(track)
        if not tracks:
            continue
        out_windows.append({
            "roundNumber": rn,
            "startTick": int(start),
            "tickStep": 1,
            "frameCount": int(len(grid)),
            "anchors": anchors,
            "players": tracks,
        })

    if not out_windows:
        return None
    return {
        "meta": {
            "tickrate": tickrate,
            "sampleRate": tickrate,
            "coordScale": COORD_SCALE,
            "angleScale": ANGLE_SCALE,
            "windowBeforeMs": window_before_ms,
            "windowAfterMs": window_after_ms,
        },
        "windows": out_windows,
    }


def _duel_track(g, grid, pidx: int) -> dict | None:
    import numpy as np

    g = g.drop_duplicates(subset="tick", keep="last").set_index("tick")
    aligned = g.reindex(grid)
    present = aligned["X"].notna().to_numpy()
    if not present.any():
        return None

    def col_ffill(name, scale=1.0):
        s = _num(aligned[name].ffill().bfill(), 0.0) * scale
        return np.round(s.to_numpy()).astype("int64")

    hp = np.round(_num(aligned["health"], 0.0).to_numpy()).astype("int64").clip(0, 100)
    hp = np.where(present, hp, 0)
    flash = np.round(_num(aligned["flash_duration"], 0.0).to_numpy() * 10).astype("int64").clip(0, 60)
    flash = np.where(present, flash, 0)

    return {
        "playerIndex": pidx,
        "x": _delta(col_ffill("X")),
        "y": _delta(col_ffill("Y")),
        "z": _delta(col_ffill("Z")),
        "yaw": _delta(col_ffill("yaw", ANGLE_SCALE)),
        "pitch": _delta(col_ffill("pitch", ANGLE_SCALE)),
        "hp": hp.tolist(),
        "flash": flash.tolist(),
    }


# ── shots.json ────────────────────────────────────────────────────────────────

def build_shots(raw: dict, players: PlayerDirectory, round_model: _RoundModel) -> dict | None:
    """Columnar weapon-fire tracks grouped by (roundNumber, playerIndex).

    Velocity is not available via weapon_fire event player extras in
    demoparser2; it is fetched from tick data (velocity_X/Y/Z) and joined
    on (tick, playerIndex).
    """
    from .events import _active_event_round_number, _safe_float

    # Build playerIndex → {tick: (vx, vy, vz)} lookup from tick data.
    # _prep_frame_df handles steamid→playerIndex mapping and tick coercion.
    # NaN velocity (data unavailable, e.g. pre-round-start) is skipped per
    # field-contract.md NaN/Infinity rule — sentinel (0,0,0) on lookup miss.
    vel_by_player: dict[int, dict[int, tuple[int, int, int]]] = {}
    vel_df = _prep_frame_df(raw.get("fire_velocity_df"), players)
    if vel_df is not None and len(vel_df) > 0:
        import numpy as np

        tick_arr = vel_df["tick"].values
        pidx_arr = vel_df["_pidx"].values
        vx_arr = vel_df["velocity_X"].values
        vy_arr = vel_df["velocity_Y"].values
        vz_arr = vel_df["velocity_Z"].values
        for pi_np, tick_np, vx_np, vy_np, vz_np in zip(
            pidx_arr, tick_arr, vx_arr, vy_arr, vz_arr,
        ):
            # NaN → data unavailable; skip so lookup falls through to sentinel (0,0,0).
            if np.isnan(vx_np) or np.isnan(vy_np) or np.isnan(vz_np):
                continue
            pi = int(pi_np)
            tick = int(tick_np)
            vel = vel_by_player.get(pi)
            if vel is None:
                vel = {}
                vel_by_player[pi] = vel
            # Last write wins if duplicate (tick, playerIndex) rows exist.
            vel[tick] = (
                int(round(float(vx_np))),
                int(round(float(vy_np))),
                int(round(float(vz_np))),
            )

    groups: dict[tuple[int, int], list[dict]] = {}
    for r in raw.get("fires", []):
        n = _active_event_round_number(round_model, r)
        if n is None:
            continue
        idx = players.idx(_sid(r.get("user_steamid") or r.get("steamid") or r.get("userid")))
        if idx is None:
            continue
        weapon = str(r.get("weapon") or "")
        if not weapon:
            continue
        groups.setdefault((n, idx), []).append(r)

    if not groups:
        return None

    weapon_dict = _Dict()
    tracks: list[dict] = []
    for (rn, idx), rows in sorted(groups.items()):
        rows.sort(key=lambda r: int(r.get("tick") or 0))

        def col(key, scale=1.0, src=rows):
            return [int(round(_safe_float(r.get(key), 0.0) * scale)) for r in src]

        vx_vals: list[int] = []
        vy_vals: list[int] = []
        vz_vals: list[int] = []
        player_vel = vel_by_player.get(idx, {})
        for r in rows:
            tick = int(r.get("tick") or 0)
            vx, vy, vz = player_vel.get(tick, (0, 0, 0))
            vx_vals.append(vx)
            vy_vals.append(vy)
            vz_vals.append(vz)

        tracks.append({
            "roundNumber": rn,
            "playerIndex": idx,
            "tick": _delta([int(r.get("tick") or 0) for r in rows]),
            "weapon": [weapon_dict.index(str(r.get("weapon"))) for r in rows],
            "x": _delta(col("user_X")),
            "y": _delta(col("user_Y")),
            "z": _delta(col("user_Z")),
            "vx": vx_vals,
            "vy": vy_vals,
            "vz": vz_vals,
            "yaw": _delta(col("user_yaw", ANGLE_SCALE)),
            "pitch": _delta(col("user_pitch", ANGLE_SCALE)),
        })

    return {
        "meta": {"coordScale": COORD_SCALE, "angleScale": ANGLE_SCALE},
        "weaponDict": weapon_dict.values,
        "tracks": tracks,
    }
