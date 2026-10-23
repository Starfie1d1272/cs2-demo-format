from __future__ import annotations

import pytest

from cs2df.events import PlayerDirectory, _annotate_trades, build_kills
from cs2df.rounds import _RoundModel, _RoundWindow


PLAYERS = PlayerDirectory([
    {"steamId64": str(76561198000000000 + i), "name": str(i), "teamKey": team}
    for i, team in enumerate(["teamA", "teamA", "teamB", "teamB", "teamA"])
])


def kill(killer, victim, tick, round_number=1):
    return {"killerIndex": killer, "victimIndex": victim, "tick": tick,
            "roundNumber": round_number, "tradeKill": False, "tradeDeath": False}


def test_unsorted_events_recover_nearest_teammate_and_clear_stale_flags():
    first, second, trade = kill(2, 0, 100), kill(2, 1, 110), kill(4, 2, 120)
    first["tradeDeath"] = True
    rows = [trade, first, second]
    _annotate_trades(rows, PLAYERS)
    assert rows == [first, second, trade]
    assert [r["tradeDeath"] for r in rows] == [False, True, False]
    assert [r["tradeKill"] for r in rows] == [False, False, True]
    _annotate_trades(rows, PLAYERS)
    assert [r["tradeDeath"] for r in rows] == [False, True, False]


@pytest.mark.parametrize("prior,current", [
    (kill(2, 0, 100, 1), kill(1, 2, 110, 2)),  # next round
    (kill(2, 2, 100), kill(0, 2, 110)),        # prior suicide
    (kill(2, 3, 100), kill(0, 2, 110)),        # prior team kill
    (kill(2, 0, 100), kill(2, 2, 110)),        # current suicide
    (kill(2, 0, 100), kill(3, 2, 110)),        # current team kill
    (kill(None, 0, 100), kill(1, 2, 110)),     # world death
    (kill(2, 0, 100), kill(None, 2, 110)),     # current world death
    (kill(2, 0, 100), kill(1, 2, 485)),        # outside six seconds
    (kill(2, 2, 101299, 12), kill(0, 2, 18017, 2)),  # observed cross-round pattern
])
def test_invalid_links_are_not_trades(prior, current):
    rows = [prior, current]
    _annotate_trades(rows, PLAYERS)
    assert not any(r["tradeKill"] or r["tradeDeath"] for r in rows)


@pytest.mark.parametrize("delta", [0, 384])
def test_window_is_inclusive_and_same_tick_order_is_stable(delta):
    prior, current = kill(2, 0, 100), kill(1, 2, 100 + delta)
    rows = [prior, current]
    _annotate_trades(rows, PLAYERS)
    assert rows == [prior, current]
    assert prior["tradeDeath"] and current["tradeKill"]


def test_unrelated_events_do_not_hide_a_trade_within_window():
    prior, current = kill(2, 0, 100), kill(1, 2, 200)
    rows = [prior, *[kill(None, 3, 110 + i) for i in range(25)], current]
    _annotate_trades(rows, PLAYERS)
    assert prior["tradeDeath"] and current["tradeKill"]


def test_builder_uses_demo_tickrate_and_sorts_parser_rows():
    model = _RoundModel(
        windows=[_RoundWindow(round_number=1, start_tick=1, freeze_end_tick=10, end_tick=2000)],
        side_map={(1, "teamA"): "t", (1, "teamB"): "ct"},
    )
    def death(killer, victim, tick):
        return {"tick": tick, "user_steamid": PLAYERS.rows[victim]["steamId64"],
                "attacker_steamid": PLAYERS.rows[killer]["steamId64"], "weapon": "ak47"}
    rows = build_kills({"tickrate": 128, "deaths": [death(1, 2, 868), death(2, 0, 100)]}, PLAYERS, model)
    assert [r["tick"] for r in rows] == [100, 868]
    assert rows[0]["tradeDeath"] and rows[1]["tradeKill"]
