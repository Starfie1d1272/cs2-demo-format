use std::collections::BTreeMap;

use serde::Serialize;

use crate::enums::{normalize_weapon_name, GrenadeType};
use crate::events::{round_ties_even_i64, DamageRow, KillRow, PlayerDirectory};
use crate::rounds::RoundModel;

pub const ANGLE_SCALE: i64 = 10;
pub const COORD_SCALE: i64 = 1;
pub const FLAG_ALIVE: i64 = 1;
pub const FLAG_HAS_BOMB: i64 = 2;
pub const FLAG_HAS_DEFUSE_KIT: i64 = 4;
pub const FLAG_HAS_HELMET: i64 = 8;
pub const FLAG_IS_SCOPED: i64 = 16;
pub const FLAG_IS_WALKING: i64 = 32;
pub const FLAG_IS_AIRBORNE: i64 = 64;
pub const FLAG_IS_DUCKING: i64 = 128;
pub const FLAG_IS_IN_RELOAD: i64 = 256;

#[derive(Debug, Default, Clone)]
pub struct StringDict {
    pub values: Vec<String>,
    idx: std::collections::HashMap<String, i32>,
}

impl StringDict {
    pub fn index(&mut self, value: Option<&str>) -> i32 {
        let Some(v) = value else {
            return -1;
        };
        if v.is_empty() {
            return -1;
        }
        if let Some(&i) = self.idx.get(v) {
            return i;
        }
        let i = self.values.len() as i32;
        self.idx.insert(v.to_string(), i);
        self.values.push(v.to_string());
        i
    }
}

pub fn delta_encode(values: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(values.len());
    let mut prev = 0i64;
    for &v in values {
        out.push(v - prev);
        prev = v;
    }
    out
}

#[derive(Debug, Default)]
pub struct ReplayColumns {
    pub ticks: Vec<i64>,
    pub steamids: Vec<Option<u64>>,
    pub x: Vec<Option<f32>>,
    pub y: Vec<Option<f32>>,
    pub z: Vec<Option<f32>>,
    pub yaw: Vec<Option<f32>>,
    pub pitch: Vec<Option<f32>>,
    pub health: Vec<Option<f32>>,
    pub armor: Vec<Option<f32>>,
    pub flash_duration: Vec<Option<f32>>,
    pub balance: Vec<Option<f32>>,
    pub current_equip_value: Vec<Option<f32>>,
    pub has_defuser: Vec<bool>,
    pub weapon_dict: StringDict,
    pub weapon_codes: Vec<i32>,
    pub place_dict: StringDict,
    pub place_codes: Vec<i32>,
    pub grenades: Vec<Vec<GrenadeType>>,
    pub has_bomb: Vec<bool>,
    pub flags_extra: Vec<i64>,
    pub ammo: Vec<i64>,
}

impl ReplayColumns {
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    pub fn push_inventory(&mut self, items: &[String]) {
        let mut grenades = Vec::new();
        let mut bomb = false;
        for item in items {
            let trimmed = item.trim();
            if let Some(g) = GrenadeType::from_inventory_item(trimmed) {
                grenades.push(g);
            } else if !bomb && normalize_weapon_name(Some(trimmed)).as_deref() == Some("c4") {
                bomb = true;
            }
        }
        self.grenades.push(grenades);
        self.has_bomb.push(bomb);
    }

    pub fn shift_ticks(&mut self, off: i64) {
        for t in &mut self.ticks {
            *t += off;
        }
    }

    pub fn max_tick(&self) -> i64 {
        self.ticks.iter().copied().max().unwrap_or(0)
    }

    pub fn extend_from(&mut self, other: ReplayColumns) {
        let w_remap: Vec<i32> = other
            .weapon_dict
            .values
            .iter()
            .map(|s| self.weapon_dict.index(Some(s)))
            .collect();
        let p_remap: Vec<i32> = other
            .place_dict
            .values
            .iter()
            .map(|s| self.place_dict.index(Some(s)))
            .collect();

        self.ticks.extend(other.ticks);
        self.steamids.extend(other.steamids);
        self.x.extend(other.x);
        self.y.extend(other.y);
        self.z.extend(other.z);
        self.yaw.extend(other.yaw);
        self.pitch.extend(other.pitch);
        self.health.extend(other.health);
        self.armor.extend(other.armor);
        self.flash_duration.extend(other.flash_duration);
        self.balance.extend(other.balance);
        self.current_equip_value.extend(other.current_equip_value);
        self.has_defuser.extend(other.has_defuser);
        self.weapon_codes
            .extend(other.weapon_codes.into_iter().map(|c| {
                if c >= 0 {
                    w_remap[c as usize]
                } else {
                    -1
                }
            }));
        self.place_codes
            .extend(other.place_codes.into_iter().map(|c| {
                if c >= 0 {
                    p_remap[c as usize]
                } else {
                    -1
                }
            }));
        self.grenades.extend(other.grenades);
        self.has_bomb.extend(other.has_bomb);
        self.flags_extra.extend(other.flags_extra);
        self.ammo.extend(other.ammo);
    }
}

#[derive(Debug, Default)]
pub struct DuelColumns {
    pub ticks: Vec<i64>,
    pub steamids: Vec<Option<u64>>,
    pub x: Vec<Option<f32>>,
    pub y: Vec<Option<f32>>,
    pub z: Vec<Option<f32>>,
    pub yaw: Vec<Option<f32>>,
    pub pitch: Vec<Option<f32>>,
    pub health: Vec<Option<f32>>,
    pub flash_duration: Vec<Option<f32>>,
    pub flags: Vec<i64>,
}

impl DuelColumns {
    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }

    pub fn shift_ticks(&mut self, off: i64) {
        for t in &mut self.ticks {
            *t += off;
        }
    }

    pub fn max_tick(&self) -> i64 {
        self.ticks.iter().copied().max().unwrap_or(0)
    }

    pub fn extend_from(&mut self, other: DuelColumns) {
        self.ticks.extend(other.ticks);
        self.steamids.extend(other.steamids);
        self.x.extend(other.x);
        self.y.extend(other.y);
        self.z.extend(other.z);
        self.yaw.extend(other.yaw);
        self.pitch.extend(other.pitch);
        self.health.extend(other.health);
        self.flash_duration.extend(other.flash_duration);
        self.flags.extend(other.flags);
    }
}

#[derive(Debug, Clone)]
pub struct FireRawRow {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub user_steamid: Option<u64>,
    pub weapon: Option<String>,
    pub user_x: Option<f64>,
    pub user_y: Option<f64>,
    pub user_z: Option<f64>,
    pub user_yaw: Option<f64>,
    pub user_pitch: Option<f64>,
    pub velocity_x: Option<f64>,
    pub velocity_y: Option<f64>,
    pub velocity_z: Option<f64>,
    pub aim_punch_x: Option<f64>,
    pub aim_punch_y: Option<f64>,
    pub accuracy_penalty: Option<f64>,
    pub recoil_idx: Option<i64>,
    pub ammo: Option<i64>,
    pub flags: i64,
}

#[derive(Debug, Clone)]
pub struct GrenadeTrajectoryRaw {
    pub grenade: GrenadeType,
    pub steamid: Option<u64>,
    pub start_tick: i64,
    pub xs: Vec<i64>,
    pub ys: Vec<i64>,
    pub zs: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayMeta {
    pub sample_rate: i64,
    pub tickrate: i64,
    pub coord_scale: i64,
    pub angle_scale: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayProjectile {
    pub grenade: GrenadeType,
    pub thrower_index: Option<usize>,
    pub start_tick: i64,
    pub x: Vec<i64>,
    pub y: Vec<i64>,
    pub z: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayPlayerTrack {
    pub player_index: usize,
    pub x: Vec<i64>,
    pub y: Vec<i64>,
    pub z: Vec<i64>,
    pub yaw: Vec<i64>,
    pub pitch: Vec<i64>,
    pub hp: Vec<i64>,
    pub armor: Vec<i64>,
    pub money: Vec<i64>,
    pub equip_value: Vec<i64>,
    pub weapon: Vec<i64>,
    pub place: Vec<i64>,
    pub flash: Vec<i64>,
    pub flags: Vec<i64>,
    pub grenades: Vec<Vec<GrenadeType>>,
    pub ammo: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayRound {
    pub round_number: i64,
    pub start_tick: i64,
    pub tick_step: i64,
    pub frame_count: usize,
    pub players: Vec<ReplayPlayerTrack>,
    pub projectiles: Vec<ReplayProjectile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayOutput {
    pub meta: ReplayMeta,
    pub weapon_dict: Vec<String>,
    pub place_dict: Vec<String>,
    pub rounds: Vec<ReplayRound>,
}

fn ffill_bfill_round(vals: &[Option<f32>], scale: f64) -> Vec<i64> {
    let n = vals.len();
    let mut out = vec![0i64; n];
    let mut last: Option<f32> = None;
    for (i, v) in vals.iter().enumerate() {
        if let Some(f) = v.filter(|x| x.is_finite()) {
            last = Some(f);
            out[i] = round_ties_even_i64(f as f64 * scale);
        } else if let Some(f) = last {
            out[i] = round_ties_even_i64(f as f64 * scale);
        }
    }
    if let Some(first_valid_idx) = vals.iter().position(|v| v.is_some_and(|x| x.is_finite())) {
        let first_val = round_ties_even_i64(vals[first_valid_idx].unwrap() as f64 * scale);
        for item in out.iter_mut().take(first_valid_idx) {
            *item = first_val;
        }
    }
    out
}

fn align_sorted_grid(rows: &[(usize, i64, usize)], grid: &[i64]) -> Vec<Option<usize>> {
    let mut out = vec![None; grid.len()];
    let mut r_pos = 0usize;
    for (fi, &gt) in grid.iter().enumerate() {
        while r_pos < rows.len() && rows[r_pos].1 < gt {
            r_pos += 1;
        }
        let mut matched = None;
        while r_pos < rows.len() && rows[r_pos].1 == gt {
            matched = Some(rows[r_pos].2);
            r_pos += 1;
        }
        out[fi] = matched;
    }
    out
}

pub fn build_replay(
    cols: &ReplayColumns,
    grenade_trajectories: &[GrenadeTrajectoryRaw],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    tickrate: i64,
    sample_rate: i64,
) -> Option<ReplayOutput> {
    if cols.is_empty() {
        return None;
    }

    let step = (tickrate / sample_rate).max(1);

    let mut by_tick: Vec<(i64, usize, usize)> = Vec::with_capacity(cols.len());
    for i in 0..cols.len() {
        let Some(pidx) = players.idx(cols.steamids[i]) else {
            continue;
        };
        by_tick.push((cols.ticks[i], pidx, i));
    }
    if by_tick.is_empty() {
        return None;
    }
    by_tick.sort_by_key(|r| (r.0, r.2));

    let mut proj_by_round: std::collections::HashMap<i64, Vec<ReplayProjectile>> =
        std::collections::HashMap::new();
    for tr in grenade_trajectories {
        let Some(rn) = round_model.round_for_tick(tr.start_tick) else {
            continue;
        };
        let thrower_index = players.idx(tr.steamid);
        proj_by_round.entry(rn).or_default().push(ReplayProjectile {
            grenade: tr.grenade,
            thrower_index,
            start_tick: tr.start_tick,
            x: delta_encode(&tr.xs),
            y: delta_encode(&tr.ys),
            z: delta_encode(&tr.zs),
        });
    }

    let mut windows = round_model.windows.clone();
    windows.sort_by_key(|w| w.round_number);

    let mut rounds_out = Vec::new();
    let mut round_rows: Vec<(usize, i64, usize)> = Vec::new();

    for w in windows {
        let Some(event_end_tick) = round_model.event_end_tick(w.round_number) else {
            continue;
        };
        if w.freeze_end_tick > event_end_tick {
            continue;
        }
        let mut grid = Vec::new();
        let mut t = w.freeze_end_tick;
        while t <= event_end_tick {
            grid.push(t);
            t += step;
        }
        if grid.is_empty() {
            continue;
        }
        let left = by_tick.partition_point(|r| r.0 < grid[0]);
        let right = by_tick.partition_point(|r| r.0 <= *grid.last().unwrap());
        if right <= left {
            continue;
        }

        round_rows.clear();
        for &(tk, pidx, ri) in &by_tick[left..right] {
            round_rows.push((pidx, tk, ri));
        }
        round_rows.sort_by_key(|r| (r.0, r.1, r.2));

        let mut player_tracks = Vec::new();
        let mut start_idx = 0usize;
        while start_idx < round_rows.len() {
            let pidx = round_rows[start_idx].0;
            let mut end_idx = start_idx + 1;
            while end_idx < round_rows.len() && round_rows[end_idx].0 == pidx {
                end_idx += 1;
            }
            let aligned = align_sorted_grid(&round_rows[start_idx..end_idx], &grid);
            start_idx = end_idx;

            if aligned.iter().all(Option::is_none) {
                continue;
            }

            let frame_count = grid.len();
            let mut present = vec![false; frame_count];
            let mut raw_x = vec![None; frame_count];
            let mut raw_y = vec![None; frame_count];
            let mut raw_z = vec![None; frame_count];
            let mut raw_yaw = vec![None; frame_count];
            let mut raw_pitch = vec![None; frame_count];
            let mut raw_money = vec![None; frame_count];
            let mut raw_equip = vec![None; frame_count];
            let mut hp = vec![0i64; frame_count];
            let mut armor = vec![0i64; frame_count];
            let mut flash = vec![0i64; frame_count];
            let mut widx = vec![-1i64; frame_count];
            let mut plidx = vec![-1i64; frame_count];
            let mut flags = vec![0i64; frame_count];
            let mut ammo = vec![0i64; frame_count];

            for (fi, row_opt) in aligned.iter().enumerate() {
                let Some(ri) = *row_opt else {
                    continue;
                };
                present[fi] = true;
                raw_x[fi] = cols.x[ri];
                raw_y[fi] = cols.y[ri];
                raw_z[fi] = cols.z[ri];
                raw_yaw[fi] = cols.yaw[ri];
                raw_pitch[fi] = cols.pitch[ri];
                raw_money[fi] = cols.balance[ri];
                raw_equip[fi] = cols.current_equip_value[ri];
                let h = round_ties_even_i64(cols.health[ri].unwrap_or(0.0) as f64).clamp(0, 100);
                hp[fi] = h;
                armor[fi] = round_ties_even_i64(cols.armor[ri].unwrap_or(0.0) as f64).clamp(0, 100);
                flash[fi] =
                    round_ties_even_i64(cols.flash_duration[ri].unwrap_or(0.0) as f64 * 10.0)
                        .clamp(0, 60);
                widx[fi] = cols.weapon_codes[ri] as i64;
                plidx[fi] = cols.place_codes[ri] as i64;
                let alive = if h > 0 { 1 } else { 0 };
                flags[fi] = alive * FLAG_ALIVE
                    + if cols.has_bomb[ri] && alive == 1 {
                        FLAG_HAS_BOMB
                    } else {
                        0
                    }
                    + if cols.has_defuser[ri] && alive == 1 {
                        FLAG_HAS_DEFUSE_KIT
                    } else {
                        0
                    }
                    + if alive == 1 {
                        cols.flags_extra.get(ri).copied().unwrap_or(0)
                    } else {
                        0
                    };
                ammo[fi] = if alive == 1 {
                    cols.ammo.get(ri).copied().unwrap_or(0)
                } else {
                    0
                };
            }

            let mut grenades_out = vec![Vec::new(); frame_count];
            let mut last_g: Option<&Vec<GrenadeType>> = None;
            let first_g = aligned.iter().position(|o| o.is_some());
            for fi in 0..frame_count {
                if let Some(ri) = aligned[fi] {
                    last_g = Some(&cols.grenades[ri]);
                }
                let chosen = last_g
                    .or_else(|| first_g.and_then(|idx| aligned[idx].map(|ri| &cols.grenades[ri])));
                if present[fi] {
                    if let Some(g) = chosen {
                        grenades_out[fi] = g.clone();
                    }
                }
            }

            player_tracks.push(ReplayPlayerTrack {
                player_index: pidx,
                x: delta_encode(&ffill_bfill_round(&raw_x, 1.0)),
                y: delta_encode(&ffill_bfill_round(&raw_y, 1.0)),
                z: delta_encode(&ffill_bfill_round(&raw_z, 1.0)),
                yaw: delta_encode(&ffill_bfill_round(&raw_yaw, ANGLE_SCALE as f64)),
                pitch: delta_encode(&ffill_bfill_round(&raw_pitch, ANGLE_SCALE as f64)),
                hp,
                armor,
                money: delta_encode(&ffill_bfill_round(&raw_money, 1.0)),
                equip_value: delta_encode(&ffill_bfill_round(&raw_equip, 1.0)),
                weapon: widx,
                place: plidx,
                flash,
                flags,
                grenades: grenades_out,
                ammo,
            });
        }

        if player_tracks.is_empty() {
            continue;
        }
        rounds_out.push(ReplayRound {
            round_number: w.round_number,
            start_tick: grid[0],
            tick_step: step,
            frame_count: grid.len(),
            players: player_tracks,
            projectiles: proj_by_round.remove(&w.round_number).unwrap_or_default(),
        });
    }

    if rounds_out.is_empty() {
        return None;
    }

    Some(ReplayOutput {
        meta: ReplayMeta {
            sample_rate: (tickrate / step).max(1),
            tickrate,
            coord_scale: COORD_SCALE,
            angle_scale: ANGLE_SCALE,
        },
        weapon_dict: cols.weapon_dict.values.clone(),
        place_dict: cols.place_dict.values.clone(),
        rounds: rounds_out,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuelsMeta {
    pub tickrate: i64,
    pub sample_rate: i64,
    pub coord_scale: i64,
    pub angle_scale: i64,
    pub window_before_ms: i64,
    pub window_after_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuelAnchor {
    pub kind: &'static str,
    pub tick: i64,
    pub attacker_index: Option<usize>,
    pub victim_index: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuelPlayerTrack {
    pub player_index: usize,
    pub x: Vec<i64>,
    pub y: Vec<i64>,
    pub z: Vec<i64>,
    pub yaw: Vec<i64>,
    pub pitch: Vec<i64>,
    pub hp: Vec<i64>,
    pub flash: Vec<i64>,
    pub flags: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuelWindowOut {
    pub round_number: i64,
    pub start_tick: i64,
    pub tick_step: i64,
    pub frame_count: usize,
    pub anchors: Vec<DuelAnchor>,
    pub players: Vec<DuelPlayerTrack>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuelsOutput {
    pub meta: DuelsMeta,
    pub windows: Vec<DuelWindowOut>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_duels(
    cols: &DuelColumns,
    duel_windows: &[(i64, i64)],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    tickrate: i64,
    kills_list: &[KillRow],
    damages_list: &[DamageRow],
    window_before_ms: i64,
    window_after_ms: i64,
) -> Option<DuelsOutput> {
    if cols.is_empty() || duel_windows.is_empty() {
        return None;
    }

    let mut by_tick: Vec<(i64, usize, usize)> = Vec::with_capacity(cols.len());
    for i in 0..cols.len() {
        let Some(pidx) = players.idx(cols.steamids[i]) else {
            continue;
        };
        by_tick.push((cols.ticks[i], pidx, i));
    }
    if by_tick.is_empty() {
        return None;
    }
    by_tick.sort_by_key(|r| (r.0, r.2));

    let mut anchors_all: Vec<DuelAnchor> =
        Vec::with_capacity(kills_list.len() + damages_list.len());
    for k in kills_list {
        anchors_all.push(DuelAnchor {
            kind: "kill",
            tick: k.tick,
            attacker_index: k.killer_index,
            victim_index: k.victim_index,
        });
    }
    for d in damages_list {
        anchors_all.push(DuelAnchor {
            kind: "damage",
            tick: d.tick,
            attacker_index: d.attacker_index,
            victim_index: d.victim_index,
        });
    }
    anchors_all.sort_by_key(|a| a.tick);

    let mut out_windows = Vec::new();
    let mut win_rows: Vec<(usize, i64, usize)> = Vec::new();
    let mut grid: Vec<i64> = Vec::new();

    for &(start, end) in duel_windows {
        let Some(rn) = round_model.round_for_tick(start) else {
            continue;
        };
        if start > end {
            continue;
        }
        let left = by_tick.partition_point(|r| r.0 < start);
        let right = by_tick.partition_point(|r| r.0 <= end);
        if right <= left {
            continue;
        }

        let a_lo = anchors_all.partition_point(|a| a.tick < start);
        let a_hi = anchors_all.partition_point(|a| a.tick <= end);
        if a_hi <= a_lo {
            continue;
        }
        let anchors = anchors_all[a_lo..a_hi].to_vec();

        grid.clear();
        grid.extend(start..=end);
        let frame_count = grid.len();

        win_rows.clear();
        for &(tk, pidx, ri) in &by_tick[left..right] {
            win_rows.push((pidx, tk, ri));
        }
        win_rows.sort_by_key(|r| (r.0, r.1, r.2));

        let mut tracks = Vec::new();
        let mut start_idx = 0usize;
        while start_idx < win_rows.len() {
            let pidx = win_rows[start_idx].0;
            let mut end_idx = start_idx + 1;
            while end_idx < win_rows.len() && win_rows[end_idx].0 == pidx {
                end_idx += 1;
            }
            let aligned = align_sorted_grid(&win_rows[start_idx..end_idx], &grid);
            start_idx = end_idx;

            let mut raw_x = vec![None; frame_count];
            let mut raw_y = vec![None; frame_count];
            let mut raw_z = vec![None; frame_count];
            let mut raw_yaw = vec![None; frame_count];
            let mut raw_pitch = vec![None; frame_count];
            let mut hp = vec![0i64; frame_count];
            let mut flash = vec![0i64; frame_count];
            let mut flags = vec![0i64; frame_count];
            let mut any_present = false;

            for (fi, row_opt) in aligned.iter().enumerate() {
                let Some(ri) = *row_opt else {
                    continue;
                };
                if cols.x[ri].is_some_and(|x| x.is_finite()) {
                    any_present = true;
                    raw_x[fi] = cols.x[ri];
                    raw_y[fi] = cols.y[ri];
                    raw_z[fi] = cols.z[ri];
                    raw_yaw[fi] = cols.yaw[ri];
                    raw_pitch[fi] = cols.pitch[ri];
                    hp[fi] =
                        round_ties_even_i64(cols.health[ri].unwrap_or(0.0) as f64).clamp(0, 100);
                    flash[fi] =
                        round_ties_even_i64(cols.flash_duration[ri].unwrap_or(0.0) as f64 * 10.0)
                            .clamp(0, 60);
                    flags[fi] = if hp[fi] > 0 {
                        cols.flags.get(ri).copied().unwrap_or(0)
                    } else {
                        0
                    };
                }
            }
            if !any_present {
                continue;
            }
            tracks.push(DuelPlayerTrack {
                player_index: pidx,
                x: delta_encode(&ffill_bfill_round(&raw_x, 1.0)),
                y: delta_encode(&ffill_bfill_round(&raw_y, 1.0)),
                z: delta_encode(&ffill_bfill_round(&raw_z, 1.0)),
                yaw: delta_encode(&ffill_bfill_round(&raw_yaw, ANGLE_SCALE as f64)),
                pitch: delta_encode(&ffill_bfill_round(&raw_pitch, ANGLE_SCALE as f64)),
                hp,
                flash,
                flags,
            });
        }

        if tracks.is_empty() {
            continue;
        }
        out_windows.push(DuelWindowOut {
            round_number: rn,
            start_tick: start,
            tick_step: 1,
            frame_count,
            anchors,
            players: tracks,
        });
    }

    if out_windows.is_empty() {
        return None;
    }

    Some(DuelsOutput {
        meta: DuelsMeta {
            tickrate,
            sample_rate: tickrate,
            coord_scale: COORD_SCALE,
            angle_scale: ANGLE_SCALE,
            window_before_ms,
            window_after_ms,
        },
        windows: out_windows,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotsMeta {
    pub coord_scale: i64,
    pub angle_scale: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotTrack {
    pub round_number: i64,
    pub player_index: usize,
    pub tick: Vec<i64>,
    pub weapon: Vec<i64>,
    pub x: Vec<i64>,
    pub y: Vec<i64>,
    pub z: Vec<i64>,
    pub vx: Vec<i64>,
    pub vy: Vec<i64>,
    pub vz: Vec<i64>,
    pub yaw: Vec<i64>,
    pub pitch: Vec<i64>,
    pub aim_punch_x: Vec<i64>,
    pub aim_punch_y: Vec<i64>,
    pub accuracy_penalty: Vec<i64>,
    pub recoil_idx: Vec<i64>,
    pub ammo: Vec<i64>,
    pub flags: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotsOutput {
    pub meta: ShotsMeta,
    pub weapon_dict: Vec<String>,
    pub tracks: Vec<ShotTrack>,
}

pub fn build_shots(
    fires: &[FireRawRow],
    players: &PlayerDirectory,
    round_model: &RoundModel,
) -> Option<ShotsOutput> {
    let mut groups: BTreeMap<(i64, usize), Vec<&FireRawRow>> = BTreeMap::new();
    for r in fires {
        let Some(n) = round_model.round_for_event(r.tick, r.total_rounds_played) else {
            continue;
        };
        if !round_model.has_round(n) {
            continue;
        }
        let Some(window) = round_model.window_for_round(n) else {
            continue;
        };
        let Some(event_end) = round_model.event_end_tick(n) else {
            continue;
        };
        if r.tick < window.freeze_end_tick || r.tick > event_end {
            continue;
        }
        let Some(idx) = players.idx(r.user_steamid) else {
            continue;
        };
        let weapon = r.weapon.as_deref().unwrap_or("");
        if weapon.is_empty() {
            continue;
        }
        groups.entry((n, idx)).or_default().push(r);
    }

    if groups.is_empty() {
        return None;
    }

    let mut weapon_dict = StringDict::default();
    let mut tracks = Vec::with_capacity(groups.len());
    for ((rn, idx), mut rows) in groups {
        rows.sort_by_key(|r| r.tick);
        let ticks: Vec<i64> = rows.iter().map(|r| r.tick).collect();
        let weapons: Vec<i64> = rows
            .iter()
            .map(|r| weapon_dict.index(r.weapon.as_deref()) as i64)
            .collect();
        let xs: Vec<i64> = rows
            .iter()
            .map(|r| round_ties_even_i64(r.user_x.filter(|v| v.is_finite()).unwrap_or(0.0)))
            .collect();
        let ys: Vec<i64> = rows
            .iter()
            .map(|r| round_ties_even_i64(r.user_y.filter(|v| v.is_finite()).unwrap_or(0.0)))
            .collect();
        let zs: Vec<i64> = rows
            .iter()
            .map(|r| round_ties_even_i64(r.user_z.filter(|v| v.is_finite()).unwrap_or(0.0)))
            .collect();
        let yaws: Vec<i64> = rows
            .iter()
            .map(|r| {
                round_ties_even_i64(
                    r.user_yaw.filter(|v| v.is_finite()).unwrap_or(0.0) * ANGLE_SCALE as f64,
                )
            })
            .collect();
        let pitches: Vec<i64> = rows
            .iter()
            .map(|r| {
                round_ties_even_i64(
                    r.user_pitch.filter(|v| v.is_finite()).unwrap_or(0.0) * ANGLE_SCALE as f64,
                )
            })
            .collect();

        let mut vx_vals = Vec::with_capacity(rows.len());
        let mut vy_vals = Vec::with_capacity(rows.len());
        let mut vz_vals = Vec::with_capacity(rows.len());
        let mut ap_x_vals = Vec::with_capacity(rows.len());
        let mut ap_y_vals = Vec::with_capacity(rows.len());
        let mut acc_vals = Vec::with_capacity(rows.len());
        let mut rec_vals = Vec::with_capacity(rows.len());
        let mut ammo_vals = Vec::with_capacity(rows.len());
        let mut flag_vals = Vec::with_capacity(rows.len());
        for r in &rows {
            vx_vals.push(round_ties_even_i64(
                r.velocity_x.filter(|v| v.is_finite()).unwrap_or(0.0),
            ));
            vy_vals.push(round_ties_even_i64(
                r.velocity_y.filter(|v| v.is_finite()).unwrap_or(0.0),
            ));
            vz_vals.push(round_ties_even_i64(
                r.velocity_z.filter(|v| v.is_finite()).unwrap_or(0.0),
            ));
            ap_x_vals.push(round_ties_even_i64(
                r.aim_punch_x.filter(|v| v.is_finite()).unwrap_or(0.0) * ANGLE_SCALE as f64,
            ));
            ap_y_vals.push(round_ties_even_i64(
                r.aim_punch_y.filter(|v| v.is_finite()).unwrap_or(0.0) * ANGLE_SCALE as f64,
            ));
            acc_vals.push(round_ties_even_i64(
                r.accuracy_penalty.filter(|v| v.is_finite()).unwrap_or(0.0) * 1000.0,
            ));
            rec_vals.push(r.recoil_idx.unwrap_or(0));
            ammo_vals.push(r.ammo.unwrap_or(0));
            flag_vals.push(r.flags);
        }

        tracks.push(ShotTrack {
            round_number: rn,
            player_index: idx,
            tick: delta_encode(&ticks),
            weapon: weapons,
            x: delta_encode(&xs),
            y: delta_encode(&ys),
            z: delta_encode(&zs),
            vx: vx_vals,
            vy: vy_vals,
            vz: vz_vals,
            yaw: delta_encode(&yaws),
            pitch: delta_encode(&pitches),
            aim_punch_x: ap_x_vals,
            aim_punch_y: ap_y_vals,
            accuracy_penalty: acc_vals,
            recoil_idx: rec_vals,
            ammo: ammo_vals,
            flags: flag_vals,
        });
    }

    Some(ShotsOutput {
        meta: ShotsMeta {
            coord_scale: COORD_SCALE,
            angle_scale: ANGLE_SCALE,
        },
        weapon_dict: weapon_dict.values,
        tracks,
    })
}
