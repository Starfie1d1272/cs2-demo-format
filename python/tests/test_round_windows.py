from __future__ import annotations

from cs2df.events import PlayerDirectory, build_kills
from cs2df.rounds import _RoundModel, _RoundWindow


def test_round_for_tick_assigns_post_round_tail_to_previous_round():
    round_model = _RoundModel(
        windows=[
            _RoundWindow(round_number=1, start_tick=50, freeze_end_tick=100, end_tick=200),
            _RoundWindow(round_number=2, start_tick=300, freeze_end_tick=360, end_tick=460),
        ],
        side_map={},
    )

    assert round_model.round_for_tick(200) == 1
    assert round_model.round_for_tick(250) == 1
    assert round_model.round_for_tick(299) == 1
    assert round_model.round_for_tick(300) == 2


def test_kills_after_round_end_before_next_start_are_kept_in_previous_round():
    players = PlayerDirectory([
        {"steamId64": "76561198000000001", "name": "attacker", "teamKey": "teamA"},
        {"steamId64": "76561198000000002", "name": "victim", "teamKey": "teamB"},
    ])
    round_model = _RoundModel(
        windows=[
            _RoundWindow(round_number=1, start_tick=50, freeze_end_tick=100, end_tick=200),
            _RoundWindow(round_number=2, start_tick=300, freeze_end_tick=360, end_tick=460),
        ],
        side_map={(1, "teamA"): "t", (1, "teamB"): "ct"},
    )
    raw = {
        "deaths": [{
            "tick": 250,
            "user_steamid": "76561198000000002",
            "attacker_steamid": "76561198000000001",
            "weapon": "ak47",
            "user_X": 1,
            "user_Y": 2,
            "user_Z": 3,
        }],
    }

    kills = build_kills(raw, players, round_model)

    assert len(kills) == 1
    assert kills[0]["roundNumber"] == 1
    assert kills[0]["tick"] == 250
