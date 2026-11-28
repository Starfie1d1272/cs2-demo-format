use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::enums::{normalize_round_end_reason, EconomyType, Side, TeamKey};

#[derive(Debug, Clone)]
pub struct RoundMarkerRow {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub winner: Option<String>,
    pub reason_str: Option<String>,
    pub reason_num: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct SideSampleRow {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub steamid: Option<u64>,
    pub team_num: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PlayerInfoRow {
    pub steamid: Option<u64>,
    pub name: Option<String>,
    pub team_num: Option<i64>,
    pub crosshair_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundWindow {
    pub round_number: i64,
    pub start_tick: i64,
    pub freeze_end_tick: i64,
    pub end_tick: i64,
    pub next_start_tick: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RoundModel {
    pub windows: Vec<RoundWindow>,
    pub side_map: HashMap<(i64, TeamKey), Side>,
    by_round: HashMap<i64, RoundWindow>,
    event_end_by_round: HashMap<i64, i64>,
    sorted_starts: Vec<i64>,
    sorted_windows: Vec<RoundWindow>,
}

impl RoundModel {
    pub fn new(windows: Vec<RoundWindow>, side_map: HashMap<(i64, TeamKey), Side>) -> Self {
        let mut by_round = HashMap::with_capacity(windows.len());
        for w in &windows {
            by_round.insert(w.round_number, w.clone());
        }
        let mut ordered = windows.clone();
        ordered.sort_by_key(|w| w.start_tick);
        let sorted_starts: Vec<i64> = ordered.iter().map(|w| w.start_tick).collect();
        let mut event_end_by_round = HashMap::with_capacity(ordered.len());
        for (i, w) in ordered.iter().enumerate() {
            let mut next_start = ordered.get(i + 1).map(|nw| nw.start_tick);
            if let (Some(ns), Some(wns)) = (next_start, w.next_start_tick) {
                next_start = Some(ns.min(wns));
            }
            let event_end = match next_start {
                Some(ns) => ns - 1,
                None => w.end_tick,
            };
            event_end_by_round.insert(w.round_number, event_end);
        }
        Self {
            windows,
            side_map,
            by_round,
            event_end_by_round,
            sorted_starts,
            sorted_windows: ordered,
        }
    }

    pub fn window_for_round(&self, round_number: i64) -> Option<&RoundWindow> {
        self.by_round.get(&round_number)
    }

    pub fn has_round(&self, round_number: i64) -> bool {
        self.by_round.contains_key(&round_number)
    }

    pub fn event_end_tick(&self, round_number: i64) -> Option<i64> {
        self.event_end_by_round.get(&round_number).copied()
    }

    pub fn round_for_tick(&self, tick: i64) -> Option<i64> {
        let idx = self.sorted_starts.partition_point(|&s| s <= tick);
        if idx == 0 {
            return None;
        }
        let window = &self.sorted_windows[idx - 1];
        let event_end = self.event_end_tick(window.round_number)?;
        if window.start_tick <= tick && tick <= event_end {
            Some(window.round_number)
        } else {
            None
        }
    }

    pub fn round_for_event(&self, tick: i64, total_rounds_played: i64) -> Option<i64> {
        if tick > 0 {
            return self.round_for_tick(tick);
        }
        let fallback = total_rounds_played + 1;
        if fallback > 0 {
            Some(fallback)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundRow {
    pub round_number: i64,
    pub start_tick: i64,
    pub freeze_end_tick: i64,
    pub end_tick: i64,
    pub team_a_side: Side,
    pub team_b_side: Side,
    pub team_a_score_before: i64,
    pub team_b_score_before: i64,
    pub team_a_economy: EconomyType,
    pub team_b_economy: EconomyType,
    pub winner_team_key: TeamKey,
    pub winner_side: Side,
    pub end_reason: String,
}

#[inline]
pub fn valid_sid64(val: Option<u64>) -> Option<u64> {
    let v = val?;
    if (10_000_000_000_000_000..100_000_000_000_000_000).contains(&v) {
        Some(v)
    } else {
        None
    }
}

pub fn resolve_round_attempts(
    round_starts: &[RoundMarkerRow],
    round_freeze_ends: &[RoundMarkerRow],
    round_ends: &[RoundMarkerRow],
) -> Vec<(RoundWindow, RoundMarkerRow, Side)> {
    let mut markers: Vec<(i64, u8, &RoundMarkerRow)> =
        Vec::with_capacity(round_starts.len() + round_freeze_ends.len() + round_ends.len());
    for (kind, rows) in [
        (0u8, round_starts),
        (1u8, round_freeze_ends),
        (2u8, round_ends),
    ] {
        for row in rows {
            if row.tick > 0 {
                markers.push((row.tick, kind, row));
            }
        }
    }
    markers.sort_by_key(|m| (m.0, m.1));

    let demo_starts: Vec<i64> = markers.iter().filter(|m| m.1 == 0).map(|m| m.0).collect();

    let mut starts: HashMap<i64, i64> = HashMap::new();
    let mut freezes: HashMap<i64, i64> = HashMap::new();
    let mut completed: BTreeMap<i64, (RoundWindow, RoundMarkerRow, Side)> = BTreeMap::new();

    for (tick, kind, row) in markers {
        let number = row.total_rounds_played + if kind != 2 { 1 } else { 0 };
        if number <= 0 {
            continue;
        }
        match kind {
            0 => {
                if starts.get(&number) == Some(&tick) {
                    continue;
                }
                let rolled_back: Vec<i64> =
                    completed.keys().copied().filter(|&n| n >= number).collect();
                for n in rolled_back {
                    completed.remove(&n);
                    starts.remove(&n);
                    freezes.remove(&n);
                }
                starts.insert(number, tick);
                freezes.remove(&number);
            }
            1 => {
                freezes.insert(number, tick);
            }
            _ => {
                let winner_str = row.winner.as_deref().unwrap_or("").to_ascii_lowercase();
                let winner_side = match winner_str.as_str() {
                    "t" | "2" => Side::T,
                    "ct" | "3" => Side::Ct,
                    _ => continue,
                };
                let freeze = freezes.get(&number).copied().unwrap_or(0);
                if tick <= freeze {
                    continue;
                }
                let next_idx = demo_starts.partition_point(|&s| s <= tick);
                let next_start = demo_starts.get(next_idx).copied();
                let window = RoundWindow {
                    round_number: number,
                    start_tick: starts.get(&number).copied().unwrap_or(0),
                    freeze_end_tick: freeze,
                    end_tick: tick,
                    next_start_tick: next_start,
                };
                completed.insert(number, (window, row.clone(), winner_side));
            }
        }
    }

    completed.into_values().collect()
}

pub fn build_rounds(
    round_starts: &[RoundMarkerRow],
    round_freeze_ends: &[RoundMarkerRow],
    round_ends: &[RoundMarkerRow],
    round_side_samples: &[SideSampleRow],
    player_info: &[PlayerInfoRow],
    team_map: &HashMap<u64, TeamKey>,
) -> (Vec<RoundRow>, RoundModel) {
    let mut team_a_score = 0i64;
    let mut team_b_score = 0i64;
    let mut out = Vec::new();
    let mut side_map: HashMap<(i64, TeamKey), Side> = HashMap::new();
    let mut windows = Vec::new();

    let attempts = resolve_round_attempts(round_starts, round_freeze_ends, round_ends);
    for (window, end, winner_side) in attempts {
        let n = window.round_number;
        let fz_tick = window.freeze_end_tick;
        let (team_a_side, team_b_side) =
            sides_for_round(round_side_samples, player_info, team_map, n, Some(fz_tick));
        side_map.insert((n, TeamKey::TeamA), team_a_side);
        side_map.insert((n, TeamKey::TeamB), team_b_side);
        let winner_key = if team_a_side == winner_side {
            TeamKey::TeamA
        } else {
            TeamKey::TeamB
        };

        if window.start_tick > 0 && fz_tick > 0 {
            out.push(RoundRow {
                round_number: n,
                start_tick: window.start_tick,
                freeze_end_tick: fz_tick,
                end_tick: window.end_tick,
                team_a_side,
                team_b_side,
                team_a_score_before: team_a_score,
                team_b_score_before: team_b_score,
                team_a_economy: EconomyType::Semi,
                team_b_economy: EconomyType::Semi,
                winner_team_key: winner_key,
                winner_side,
                end_reason: normalize_round_end_reason(end.reason_str.as_deref(), end.reason_num)
                    .to_string(),
            });
            windows.push(window);
        }

        match winner_key {
            TeamKey::TeamA => team_a_score += 1,
            TeamKey::TeamB => team_b_score += 1,
        }
    }

    (out, RoundModel::new(windows, side_map))
}

fn sides_for_round(
    round_side_samples: &[SideSampleRow],
    player_info: &[PlayerInfoRow],
    team_map: &HashMap<u64, TeamKey>,
    round_number: i64,
    freeze_end_tick: Option<i64>,
) -> (Side, Side) {
    if let Some(sampled) =
        sampled_side_for_round(round_side_samples, team_map, round_number, freeze_end_tick)
    {
        return sampled;
    }

    let (team_a_initial, _) = starting_side_by_team(player_info, team_map);
    let team_a_side = if round_number <= 12 {
        team_a_initial
    } else if round_number <= 24 {
        team_a_initial.opposite()
    } else {
        let ot_block = (round_number - 25) / 3;
        if ot_block % 2 == 0 {
            team_a_initial.opposite()
        } else {
            team_a_initial
        }
    };
    (team_a_side, team_a_side.opposite())
}

fn sampled_side_for_round(
    round_side_samples: &[SideSampleRow],
    team_map: &HashMap<u64, TeamKey>,
    round_number: i64,
    freeze_end_tick: Option<i64>,
) -> Option<(Side, Side)> {
    let expected_tick = freeze_end_tick.unwrap_or(0) + 16;
    let mut a_t = 0usize;
    let mut a_ct = 0usize;
    let mut b_t = 0usize;
    let mut b_ct = 0usize;

    for row in round_side_samples {
        if expected_tick > 16 && row.tick != expected_tick {
            continue;
        }
        if expected_tick <= 16
            && row.total_rounds_played != 0
            && row.total_rounds_played + 1 != round_number
        {
            continue;
        }
        let Some(sid) = valid_sid64(row.steamid) else {
            continue;
        };
        let Some(&key) = team_map.get(&sid) else {
            continue;
        };
        let side = match row.team_num {
            Some(2) => Side::T,
            Some(3) => Side::Ct,
            _ => continue,
        };
        match (key, side) {
            (TeamKey::TeamA, Side::T) => a_t += 1,
            (TeamKey::TeamA, Side::Ct) => a_ct += 1,
            (TeamKey::TeamB, Side::T) => b_t += 1,
            (TeamKey::TeamB, Side::Ct) => b_ct += 1,
        }
    }

    let team_a_side = majority_side(a_t, a_ct);
    let team_b_side = majority_side(b_t, b_ct);
    match (team_a_side, team_b_side) {
        (Some(a), Some(b)) if a != b => Some((a, b)),
        (Some(a), _) => Some((a, a.opposite())),
        (None, Some(b)) => Some((b.opposite(), b)),
        (None, None) => None,
    }
}

fn majority_side(t: usize, ct: usize) -> Option<Side> {
    if t == ct {
        None
    } else if t > ct {
        Some(Side::T)
    } else {
        Some(Side::Ct)
    }
}

fn starting_side_by_team(
    player_info: &[PlayerInfoRow],
    team_map: &HashMap<u64, TeamKey>,
) -> (Side, Side) {
    let mut a_t = 0usize;
    let mut a_ct = 0usize;
    let mut b_t = 0usize;
    let mut b_ct = 0usize;

    for row in player_info {
        let Some(sid) = valid_sid64(row.steamid) else {
            continue;
        };
        let Some(&key) = team_map.get(&sid) else {
            continue;
        };
        match (key, row.team_num) {
            (TeamKey::TeamA, Some(2)) => a_t += 1,
            (TeamKey::TeamA, Some(3)) => a_ct += 1,
            (TeamKey::TeamB, Some(2)) => b_t += 1,
            (TeamKey::TeamB, Some(3)) => b_ct += 1,
            _ => {}
        }
    }

    (
        if a_t >= a_ct { Side::T } else { Side::Ct },
        if b_t >= b_ct { Side::T } else { Side::Ct },
    )
}
