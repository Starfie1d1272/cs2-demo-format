"""Merge split GOTV recordings of ONE match into a single parse_demo() output.

HLTV occasionally splits one match's GOTV demo into parts (``…-p1.dem``,
``…-p2.dem``). Across parts the round COUNTER is global (part 2 already starts at
round 3), but the TICK clock restarts from ~0 in every file. We therefore:

1. parse each part with :func:`cs2df.parse.parse_demo`;
2. shift every tick in each later part past the previous parts' span, so the
   merged timeline is strictly monotonic (within-part tick deltas — and thus
   reaction times, round spans, duel windows — are preserved by a constant shift);
3. concatenate the event streams and per-frame DataFrames;
4. keep match metadata / roster from the first part (the true match start).

The merged dict has the exact shape :func:`parse_demo` returns, so
:func:`cs2df.package.build_package` re-derives coherent per-match artifacts
(player-stats, clutches, …) over the union — no per-artifact merge logic needed.
"""

from __future__ import annotations

from typing import Any

# Metadata / roster taken from the first part, never shifted or concatenated.
_META_FIRST = {
    "_timings", "header", "tickrate", "sample_rate", "match_start_tick",
    "team_a_name", "team_b_name", "player_info",
}
# Event lists: list[dict]; shift any of these tick-bearing keys present on a row.
_EVENT_LISTS = {
    "round_starts", "round_freeze_ends", "round_ends",
    "deaths", "hurts", "fires", "blinds",
    "bomb_planted", "bomb_defused", "bomb_exploded", "bomb_beginplant",
    "bomb_begindefuse", "bomb_dropped", "bomb_pickup",
    "grenade_detonations", "inferno_expires", "smoke_expires",
    "grenade_throws", "grenade_trajectories",
    "round_side_samples", "economy_raw",
}
_ROW_TICK_KEYS = ("tick", "destroy_tick", "start_tick")
# Round-structure lists are ownership-filtered, never tick-trimmed; the rest are
# combat/effect events that get trimmed of a resumed part's pre-live residue.
_ROUND_LISTS = {"round_starts", "round_freeze_ends", "round_ends"}
_DF_KEYS = {"fire_velocity_df", "replay_df", "duel_df"}
_INT_TICK_LISTS = {"replay_ticks", "freeze_ticks"}
_WINDOW_LISTS = {"duel_windows"}  # list[tuple[start, end]]

# Inter-part gap, in seconds, inserted between segments so spans never touch.
_GAP_SECONDS = 8


def _round_of(row: dict[str, Any], is_end: bool) -> int:
    """Actual round number for a round_* event from its total_rounds_played.

    total_rounds_played at round_end == that round's number; at round_start /
    round_freeze_end it is N-1 for round N. Mirrors cs2df.rounds._rn semantics.
    """
    trp = int(row.get("total_rounds_played") or 0)
    return trp if is_end else trp + 1


def _max_tick(raw: dict[str, Any]) -> int:
    """Largest tick observed in a part (used to compute the next part's offset)."""
    cands = [0]
    for key in ("round_ends", "round_freeze_ends", "grenade_throws"):
        for row in raw.get(key) or []:
            for tk in ("tick", "destroy_tick"):
                v = row.get(tk)
                if isinstance(v, (int, float)) and v > 0:
                    cands.append(int(v))
    for key in _INT_TICK_LISTS:
        cands.extend(int(t) for t in (raw.get(key) or []) if t)
    for key in _DF_KEYS:
        df = raw.get(key)
        if df is not None and hasattr(df, "columns") and "tick" in getattr(df, "columns", []) and len(df):
            cands.append(int(df["tick"].max()))
    return max(cands)


def _shift_part(raw: dict[str, Any], off: int) -> None:
    """Add ``off`` to every tick in ``raw`` in place (off == 0 → no-op)."""
    if off == 0:
        return
    for key in _EVENT_LISTS:
        for row in raw.get(key) or []:
            for tk in _ROW_TICK_KEYS:
                v = row.get(tk)
                if isinstance(v, (int, float)) and v > 0:
                    row[tk] = int(v) + off
    for key in _DF_KEYS:
        df = raw.get(key)
        if df is not None and hasattr(df, "columns") and "tick" in getattr(df, "columns", []):
            df = df.copy()
            df["tick"] = df["tick"] + off
            raw[key] = df
    for key in _INT_TICK_LISTS:
        lst = raw.get(key)
        if lst:
            raw[key] = [int(t) + off for t in lst]
    for key in _WINDOW_LISTS:
        lst = raw.get(key)
        if lst:
            raw[key] = [(int(s) + off, int(e) + off) for s, e in lst]


def merge_raw(parts: list[dict[str, Any]]) -> dict[str, Any]:
    """Merge per-part parse_demo() outputs into one (parts in play order)."""
    if not parts:
        raise ValueError("merge_raw: no parts")
    if len(parts) == 1:
        return parts[0]

    # 1. shift each part past the previous parts' span.
    running = 0
    for part in parts:
        _shift_part(part, running)
        running += _max_tick(part) + part.get("tickrate", 64) * _GAP_SECONDS

    # 1b. round ownership: a part only owns rounds past the previous parts'. When a
    # GOTV recording resumes it re-emits a few warmup/restart round_* events with low
    # total_rounds_played; those would collide in build_rounds' per-round tick maps
    # (e.g. overwriting an earlier part's freeze tick and getting the seam round
    # dropped as bogus). Drop each later part's round_* events for already-owned rounds.
    prev_max = 0
    for idx, part in enumerate(parts):
        if idx > 0:
            for key, is_end in (("round_ends", True), ("round_starts", False),
                                ("round_freeze_ends", False)):
                rows = part.get(key) or []
                part[key] = [r for r in rows if _round_of(r, is_end) > prev_max]
            # Trim combat/effect residue captured before this part's first live round
            # (a resume starts mid-freeze and re-emits seam events the previous part
            # already owns). Threshold = first owned round's freeze-end tick.
            freezes = [int(r.get("tick") or 0) for r in part.get("round_freeze_ends") or []
                       if int(r.get("tick") or 0) > 0]
            trim_before = min(freezes) if freezes else 0
            if trim_before > 0:
                for key in _EVENT_LISTS - _ROUND_LISTS:
                    rows = part.get(key) or []
                    part[key] = [r for r in rows
                                 if not (0 < int(r.get("tick") or 0) < trim_before)]
        ends = part.get("round_ends") or []
        if ends:
            prev_max = max([prev_max] + [_round_of(r, True) for r in ends])

    # 2. assemble the merged dict.
    merged: dict[str, Any] = {}
    base = parts[0]
    for key in _META_FIRST:
        if key in base:
            merged[key] = base[key]

    for key in _EVENT_LISTS:
        out: list[Any] = []
        for part in parts:
            out.extend(part.get(key) or [])
        merged[key] = out

    import pandas as pd  # lazy: native dep
    for key in _DF_KEYS:
        frames = [part[key] for part in parts
                  if part.get(key) is not None and len(part[key])]
        merged[key] = pd.concat(frames, ignore_index=True) if frames else base.get(key)

    for key in _INT_TICK_LISTS:
        seen: set[int] = set()
        for part in parts:
            seen.update(int(t) for t in (part.get(key) or []))
        merged[key] = sorted(seen)

    for key in _WINDOW_LISTS:
        out_w: list[Any] = []
        for part in parts:
            out_w.extend(part.get(key) or [])
        merged[key] = out_w

    return merged


def parse_and_merge(dem_paths: list[str], **parse_kwargs: Any) -> dict[str, Any]:
    """Parse each part with parse_demo() and merge into one raw dict."""
    from .parse import parse_demo
    progress = parse_kwargs.pop("progress", None)
    n = len(dem_paths)
    parts: list[dict[str, Any]] = []
    for i, path in enumerate(dem_paths):
        def _scaled(stage: str, frac: float, _i=i) -> None:
            if progress is not None:
                progress(f"[part {_i + 1}/{n}] {stage}", (_i + frac) / n)
        parts.append(parse_demo(path, progress=_scaled if progress else None, **parse_kwargs))
    return merge_raw(parts)
