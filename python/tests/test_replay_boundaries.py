from __future__ import annotations

import pandas as pd

from cs2df.events import PlayerDirectory
from cs2df.parse import _build_sample_ticks
from cs2df.rounds import _RoundModel, _RoundWindow
from cs2df.streams import build_replay


def _replay_row(tick: int) -> dict:
    return {
        "steamid": "76561198000000001",
        "tick": tick,
        "active_weapon_name": "ak47",
        "last_place_name": "Middle",
        "inventory": [],
        "X": tick,
        "Y": tick + 1,
        "Z": tick + 2,
        "yaw": 90,
        "pitch": 0,
        "balance": 800,
        "current_equip_value": 2700,
        "health": 100,
        "armor": 100,
        "flash_duration": 0,
        "has_defuser": False,
    }


def test_replay_round_extends_to_sample_before_next_round_start():
    players = PlayerDirectory([
        {"steamId64": "76561198000000001", "name": "p1", "teamKey": "teamA"},
    ])
    round_model = _RoundModel(
        windows=[
            _RoundWindow(round_number=1, start_tick=50, freeze_end_tick=100, end_tick=200),
            _RoundWindow(round_number=2, start_tick=300, freeze_end_tick=360, end_tick=460),
        ],
        side_map={},
    )
    raw = {
        "replay_df": pd.DataFrame([_replay_row(tick) for tick in range(100, 360, 16)]),
        "grenade_trajectories": [],
    }

    replay = build_replay(raw, players, round_model, tickrate=128, sample_rate=8)

    assert replay is not None
    first_round = replay["rounds"][0]
    last_tick = first_round["startTick"] + (first_round["frameCount"] - 1) * first_round["tickStep"]
    assert last_tick == 292
    assert last_tick < 300


def test_replay_marks_round_start_bomb_carrier_from_inventory():
    players = PlayerDirectory([
        {"steamId64": "76561198000000001", "name": "p1", "teamKey": "teamA"},
    ])
    round_model = _RoundModel(
        windows=[
            _RoundWindow(round_number=1, start_tick=50, freeze_end_tick=100, end_tick=200),
            _RoundWindow(round_number=2, start_tick=300, freeze_end_tick=360, end_tick=460),
        ],
        side_map={},
    )
    rows = [_replay_row(tick) for tick in range(360, 461, 16)]
    for row in rows:
        row["inventory"] = ["C4 Explosive"]
    raw = {
        "replay_df": pd.DataFrame(rows),
        "grenade_trajectories": [],
    }

    replay = build_replay(raw, players, round_model, tickrate=128, sample_rate=8)

    assert replay is not None
    second_round = replay["rounds"][0]
    assert second_round["roundNumber"] == 2
    assert all(flags & 2 for flags in second_round["players"][0]["flags"])


def test_parse_replay_ticks_include_post_round_tail_before_next_start():
    round_ends = [
        {"total_rounds_played": 1, "tick": 200},
        {"total_rounds_played": 2, "tick": 460},
    ]
    round_freeze_ends = [
        {"total_rounds_played": 0, "tick": 100},
        {"total_rounds_played": 1, "tick": 360},
    ]
    round_starts = [
        {"total_rounds_played": 0, "tick": 50},
        {"total_rounds_played": 1, "tick": 300},
    ]

    ticks = _build_sample_ticks(round_ends, round_freeze_ends, round_starts, step=16)

    assert 292 in ticks
    assert 300 not in ticks
