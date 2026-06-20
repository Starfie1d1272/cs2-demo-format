"""Assemble a cs2-demo-format v3 ZIP package from a parsed demo."""

from __future__ import annotations

import hashlib
import io
import math
import time
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

from . import EXPORTER_NAME, SCHEMA_VERSION, __version__
from .events import (
    PlayerDirectory, build_blinds, build_bombs, build_clutches, build_damages,
    build_economies, build_grenades, build_kills, build_match,
    build_player_stats, build_players,
)
from .rounds import build_rounds
from .streams import build_duels, build_replay, build_shots

ProgressFn = Callable[[str, float], None]

_FILENAMES = {
    "match": "match.json",
    "players": "players.json",
    "rounds": "rounds.json",
    "playerStats": "player-stats.json",
    "playerEconomies": "player-economies.json",
    "kills": "kills.json",
    "damages": "damages.json",
    "blinds": "blinds.json",
    "bombs": "bombs.json",
    "grenades": "grenades.json",
    "clutches": "clutches.json",
    "shots": "shots.json",
    "replay": "replay.json",
    "duels": "duels.json",
}


def export_demo(dem_path: str, *, research: bool = False, sample_rate: int = 8,
                window_before_ms: int = 2000, window_after_ms: int = 1000,
                compress_level: int = 3,
                progress: ProgressFn | None = None) -> tuple[bytes, dict]:
    """Parse `dem_path` and return (v3 ZIP bytes, match_meta dict)."""
    from .parse import parse_demo

    try:
        demo_hash: str | None = _sha256_hex(dem_path)
    except Exception:
        demo_hash = None

    scaled = (lambda stage, frac: progress(stage, frac * 0.9)) if progress else None
    raw = parse_demo(dem_path, sample_rate=sample_rate, research=research,
                     window_before_ms=window_before_ms,
                     window_after_ms=window_after_ms, progress=scaled)

    if progress:
        progress("build package", 0.92)
    return build_package(raw, dem_path, demo_hash,
                         window_before_ms=window_before_ms,
                         window_after_ms=window_after_ms,
                         compress_level=compress_level)


def export_demo_merged(dem_paths: list[str], *, research: bool = False, sample_rate: int = 8,
                       window_before_ms: int = 2000, window_after_ms: int = 1000,
                       compress_level: int = 3,
                       progress: ProgressFn | None = None) -> tuple[bytes, dict]:
    """Parse several split GOTV parts of ONE match, merge, and build a single ZIP.

    Use for HLTV demos split into ``…-p1.dem`` / ``…-p2.dem``: ticks are reconciled
    onto a global timeline so the package's per-match derivations stay coherent.
    """
    if len(dem_paths) == 1:
        return export_demo(dem_paths[0], research=research, sample_rate=sample_rate,
                           window_before_ms=window_before_ms, window_after_ms=window_after_ms,
                           compress_level=compress_level, progress=progress)
    from .merge import parse_and_merge

    # Combined hash over all parts, in order, so the merged package has a stable id.
    try:
        h = hashlib.sha256()
        for part in dem_paths:
            h.update(_sha256_hex(part).encode("ascii"))
        demo_hash: str | None = h.hexdigest()
    except Exception:
        demo_hash = None

    scaled = (lambda stage, frac: progress(stage, frac * 0.9)) if progress else None
    raw = parse_and_merge(list(dem_paths), sample_rate=sample_rate, research=research,
                          window_before_ms=window_before_ms,
                          window_after_ms=window_after_ms, progress=scaled)
    if progress:
        progress("build package", 0.92)
    return build_package(raw, dem_paths[0], demo_hash,
                         window_before_ms=window_before_ms,
                         window_after_ms=window_after_ms,
                         compress_level=compress_level)


def build_package(raw: dict[str, Any], dem_path: str, demo_hash: str | None, *,
                  window_before_ms: int = 2000, window_after_ms: int = 1000,
                  compress_level: int = 3) -> tuple[bytes, dict]:
    """Pure assembly: raw parse output → (v3 ZIP bytes, match_meta)."""
    timings = dict(raw.get("_timings") or {})
    tickrate = int(raw.get("tickrate") or 64)
    sample_rate = int(raw.get("sample_rate") or 8)

    players = _timed(timings, "package.players", lambda: build_players(raw))
    team_map = {p["steamId64"]: p["teamKey"] for p in players.rows}
    rounds, round_model = _timed(timings, "package.rounds", lambda: build_rounds(raw, team_map))

    def build_events():
        kills_out = build_kills(raw, players, round_model)
        grenades_out = build_grenades(raw, players, round_model)
        flash_lookup = {
            (g["roundNumber"], g["effectTick"]): g["grenadeId"]
            for g in grenades_out if g["grenade"] == "flashbang" and g["grenadeId"]
        }
        blinds_out = build_blinds(raw, players, round_model, flash_lookup=flash_lookup)
        damages_out = build_damages(raw, players, round_model)
        clutches_out = build_clutches(kills_out, rounds, players)
        economies_out = build_economies(raw, players, round_model, rounds)  # mutates rounds economy
        player_stats_out = build_player_stats(
            raw, players, round_model, rounds,
            kills_list=kills_out, blinds_list=blinds_out,
            damages_list=damages_out, clutches_list=clutches_out,
        )
        bombs_out = build_bombs(raw, players, round_model)
        match_out = build_match(raw, rounds)
        return (kills_out, grenades_out, blinds_out, damages_out, clutches_out,
                economies_out, player_stats_out, bombs_out, match_out)

    (kills, grenades, blinds, damages, clutches,
     economies, player_stats, bombs, match_json) = _timed(timings, "package.events", build_events)

    shots = _timed(timings, "package.shots", lambda: build_shots(raw, players, round_model))
    replay = _timed(timings, "package.replay", lambda: build_replay(raw, players, round_model,
                                                                    tickrate, sample_rate))
    duels = _timed(timings, "package.duels", lambda: build_duels(
        raw, players, round_model, tickrate, kills, damages, window_before_ms, window_after_ms))

    data_by_key: dict[str, Any] = {
        "match": match_json,
        "players": players.rows,
        "rounds": rounds,
        "playerStats": player_stats,
        "playerEconomies": economies,
        "kills": kills,
        "damages": damages,
        "blinds": blinds,
        "bombs": bombs,
        "grenades": grenades,
        "clutches": clutches,
    }
    if shots:
        data_by_key["shots"] = shots
    if replay:
        data_by_key["replay"] = replay
    if duels:
        data_by_key["duels"] = duels

    manifest = {
        "schemaVersion": SCHEMA_VERSION,
        "exporter": {"name": EXPORTER_NAME, "version": __version__},
        "parser": {"name": "demoparser2", "version": _demoparser2_version()},
        "demo": {"hash": demo_hash, "sourceFileName": Path(dem_path).name},
        "mapName": str(raw.get("header", {}).get("map_name") or "unknown"),
        "tickrate": tickrate,
        "exportedAt": datetime.now(timezone.utc).isoformat(),
        "files": {key: _FILENAMES[key] for key in data_by_key},
    }

    match_meta = {
        "mapName": match_json.get("mapName", "unknown"),
        "teamA": match_json.get("teamA") or {},
        "teamB": match_json.get("teamB") or {},
    }
    zip_bytes = _timed(timings, "package.writeZip",
                       lambda: _write_zip(manifest, data_by_key, compress_level=compress_level))
    match_meta["timingsSeconds"] = timings
    match_meta["compressLevel"] = compress_level
    return zip_bytes, match_meta


def _write_zip(manifest: dict, data_by_key: dict[str, Any], *,
               compress_level: int = 3) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED,
                         compresslevel=compress_level) as zf:
        zf.writestr("manifest.json", _dumps(manifest))
        for key, data in data_by_key.items():
            zf.writestr(_FILENAMES[key], _dumps(data))
    return buf.getvalue()


def _timed(timings: dict[str, float], name: str, fn):
    started = time.perf_counter()
    result = fn()
    timings[name] = round(time.perf_counter() - started, 3)
    return result


def _dumps(data: Any) -> bytes:
    """Minified JSON via orjson (fast, no NaN — raises on non-finite floats)."""
    import orjson

    return orjson.dumps(_json_safe(data))


def _json_safe(obj):
    """Replace float NaN/inf with None (orjson would raise otherwise)."""
    if isinstance(obj, float):
        return None if (math.isnan(obj) or math.isinf(obj)) else obj
    if isinstance(obj, list):
        return [_json_safe(v) for v in obj]
    if isinstance(obj, dict):
        return {k: _json_safe(v) for k, v in obj.items()}
    return obj


def _sha256_hex(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _demoparser2_version() -> str:
    try:
        from importlib.metadata import version

        return version("demoparser2")
    except Exception:
        return "unknown"
