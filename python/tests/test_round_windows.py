from __future__ import annotations

from cs2df.events import PlayerDirectory, build_damages, build_kills
from cs2df.parse import _build_sample_ticks
from cs2df.rounds import _RoundModel, _RoundWindow, build_rounds


def test_round_for_tick_assigns_post_round_tail_to_previous_round():
    round_model = _RoundModel(
        windows=[
            _RoundWindow(
                round_number=1, start_tick=50, freeze_end_tick=100, end_tick=200
            ),
            _RoundWindow(
                round_number=2, start_tick=300, freeze_end_tick=360, end_tick=460
            ),
        ],
        side_map={},
    )

    assert round_model.round_for_tick(200) == 1
    assert round_model.round_for_tick(250) == 1
    assert round_model.round_for_tick(299) == 1
    assert round_model.round_for_tick(300) == 2


def test_kills_after_round_end_before_next_start_are_kept_in_previous_round():
    players = PlayerDirectory(
        [
            {"steamId64": "76561198000000001", "name": "attacker", "teamKey": "teamA"},
            {"steamId64": "76561198000000002", "name": "victim", "teamKey": "teamB"},
        ]
    )
    round_model = _RoundModel(
        windows=[
            _RoundWindow(
                round_number=1, start_tick=50, freeze_end_tick=100, end_tick=200
            ),
            _RoundWindow(
                round_number=2, start_tick=300, freeze_end_tick=360, end_tick=460
            ),
        ],
        side_map={(1, "teamA"): "t", (1, "teamB"): "ct"},
    )
    raw = {
        "deaths": [
            {
                "tick": 250,
                "user_steamid": "76561198000000002",
                "attacker_steamid": "76561198000000001",
                "weapon": "ak47",
                "user_X": 1,
                "user_Y": 2,
                "user_Z": 3,
            }
        ],
    }

    kills = build_kills(raw, players, round_model)

    assert len(kills) == 1
    assert kills[0]["roundNumber"] == 1
    assert kills[0]["tick"] == 250


def test_round_restart_keeps_previous_result_and_excludes_abandoned_ticks():
    starts = [
        {"total_rounds_played": 0, "tick": 10},
        {"total_rounds_played": 1, "tick": 100},
        {"total_rounds_played": 1, "tick": 160},
    ]
    freezes = [
        {"total_rounds_played": 0, "tick": 20},
        {"total_rounds_played": 1, "tick": 170},
    ]
    ends = [
        {"total_rounds_played": 1, "tick": 90, "winner": "ct"},
        {"total_rounds_played": 1, "tick": 160, "winner": None},
        {"total_rounds_played": 2, "tick": 200, "winner": "ct"},
    ]

    rounds, model = build_rounds(
        {"round_starts": starts, "round_freeze_ends": freezes, "round_ends": ends}, {}
    )

    assert [r["roundNumber"] for r in rounds] == [1, 2]
    assert (rounds[1]["startTick"], rounds[1]["freezeEndTick"]) == (160, 170)
    assert model.round_for_tick(99) == 1
    assert model.round_for_tick(100) is None
    assert model.round_for_tick(159) is None
    assert model.round_for_tick(160) == 2
    ticks = _build_sample_ticks(ends, freezes, starts, step=1)
    assert all(not 100 <= tick < 170 for tick in ticks)
    assert 99 in ticks and 170 in ticks

    attacker = "76561198000000001"
    victim = "76561198000000002"
    players = PlayerDirectory(
        [
            {"steamId64": attacker, "name": "attacker", "teamKey": "teamA"},
            {"steamId64": victim, "name": "victim", "teamKey": "teamB"},
        ]
    )
    damage = {
        "user_steamid": victim,
        "attacker_steamid": attacker,
        "weapon": "ak47",
        "dmg_health": 50,
    }
    damages = build_damages(
        {"hurts": [{**damage, "tick": tick} for tick in (130, 160, 180)]},
        players,
        model,
    )
    assert [(row["tick"], row["victimHealthBefore"]) for row in damages] == [(180, 100)]


def test_rollback_replaces_scored_attempt_and_discards_later_old_rounds():
    raw = {
        "round_starts": [
            {"total_rounds_played": 0, "tick": 10},
            {"total_rounds_played": 1, "tick": 100},
            {"total_rounds_played": 2, "tick": 210},
            {"total_rounds_played": 1, "tick": 260},
        ],
        "round_freeze_ends": [
            {"total_rounds_played": 0, "tick": 20},
            {"total_rounds_played": 1, "tick": 110},
            {"total_rounds_played": 2, "tick": 220},
            {"total_rounds_played": 1, "tick": 270},
        ],
        "round_ends": [
            {"total_rounds_played": 1, "tick": 90, "winner": "ct"},
            {"total_rounds_played": 2, "tick": 200, "winner": "t"},
            {"total_rounds_played": 3, "tick": 250, "winner": "t"},
            {"total_rounds_played": 1, "tick": 260, "winner": None},
            {"total_rounds_played": 2, "tick": 300, "winner": "ct"},
        ],
    }

    rounds, model = build_rounds(raw, {})

    assert [r["roundNumber"] for r in rounds] == [1, 2]
    assert (rounds[1]["startTick"], rounds[1]["freezeEndTick"]) == (260, 270)
    assert (rounds[1]["endTick"], rounds[1]["winnerSide"]) == (300, "ct")
    assert model.round_for_tick(150) is None
    assert model.round_for_tick(230) is None
    assert model.round_for_tick(270) == 2
