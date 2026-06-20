from __future__ import annotations

from cs2df.merge import merge_raw


def _part(tickrate, round_ends, round_starts, round_freeze_ends, hurts):
    return {
        "tickrate": tickrate,
        "round_ends": round_ends,
        "round_starts": round_starts,
        "round_freeze_ends": round_freeze_ends,
        "hurts": hurts,
        "deaths": [],
    }


def test_merge_two_parts_keeps_seam_round_shifts_ticks_and_trims_residue():
    # Part 0 owns rounds 1-2 (total_rounds_played: end == round number).
    part0 = _part(
        64,
        round_ends=[{"total_rounds_played": 1, "tick": 200},
                    {"total_rounds_played": 2, "tick": 460}],
        round_starts=[{"total_rounds_played": 0, "tick": 50},
                      {"total_rounds_played": 1, "tick": 300}],
        round_freeze_ends=[{"total_rounds_played": 0, "tick": 100},
                           {"total_rounds_played": 1, "tick": 360}],
        hurts=[{"tick": 150}, {"tick": 400}],
    )
    # Part 1 owns rounds 3-4 with a LOCAL tick clock (restarts near 0) and resume
    # residue: spurious low-trp round_* events (actual round 2, already owned) plus a
    # phantom hurt at the very first tick.
    part1 = _part(
        64,
        round_ends=[{"total_rounds_played": 2, "tick": 1},     # spurious (round 2)
                    {"total_rounds_played": 3, "tick": 200},
                    {"total_rounds_played": 4, "tick": 460}],
        round_starts=[{"total_rounds_played": 1, "tick": 2},   # spurious (round 2)
                      {"total_rounds_played": 2, "tick": 5},
                      {"total_rounds_played": 3, "tick": 300}],
        round_freeze_ends=[{"total_rounds_played": 1, "tick": 3},  # spurious
                           {"total_rounds_played": 2, "tick": 60},
                           {"total_rounds_played": 3, "tick": 360}],
        hurts=[{"tick": 4}, {"tick": 120}, {"tick": 400}],  # tick 4 == resume residue
    )

    merged = merge_raw([part0, part1])

    # Seam round 2 survives; rounds are 1..4 with no gap/duplicate.
    end_rounds = [int(r["total_rounds_played"]) for r in merged["round_ends"]]
    assert end_rounds == [1, 2, 3, 4]

    # Round ticks are strictly monotonic across the seam (part 1 shifted past part 0).
    end_ticks = [r["tick"] for r in merged["round_ends"]]
    assert end_ticks == sorted(end_ticks) and len(set(end_ticks)) == len(end_ticks)
    assert end_ticks[1] < end_ticks[2]  # part0's round 2 < part1's round 3

    # Resume residue (the tick-4 phantom hurt) is trimmed; the 4 real hurts remain.
    assert len(merged["hurts"]) == 4
    # No hurt sits in the dead gap between part0's last tick and part1's first live round.
    assert all(h["tick"] <= 460 or h["tick"] >= end_ticks[1] for h in merged["hurts"])


def test_single_part_passthrough():
    part = _part(64, [{"total_rounds_played": 1, "tick": 200}],
                 [{"total_rounds_played": 0, "tick": 50}],
                 [{"total_rounds_played": 0, "tick": 100}],
                 [{"tick": 150}])
    assert merge_raw([part]) is part
