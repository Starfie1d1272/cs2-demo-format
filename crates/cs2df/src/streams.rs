use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

use crate::enums::{classify_inventory, normalize_weapon_name};
use crate::events::{round_ties_even_i64, DamageRow, KillRow, PlayerDirectory};
use crate::rounds::{format_sid, RoundModel};

pub const ANGLE_SCALE: i64 = 10;
pub const COORD_SCALE: i64 = 1;
pub const FLAG_ALIVE: i64 = 1;
pub const FLAG_HAS_BOMB: i64 = 2;
pub const FLAG_HAS_DEFUSE_KIT: i64 = 4;

#[derive(Debug, Default)]
pub struct StringDict {
    pub values: Vec<String>,
    idx: HashMap<String, usize>,
}

impl StringDict {
    pub fn index(&mut self, value: Option<&str>) -> i64 {
        let Some(v) = value else {
            return -1;
        };
        if v.is_empty() {
            return -1;
        }
        if let Some(&i) = self.idx.get(v) {
            return i as i64;
        }
        let i = self.values.len();
        self.idx.insert(v.to_string(), i);
        self.values.push(v.to_string());
        i as i64
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

#[derive(Debug, Clone)]
pub struct ReplayFrameRow {
    pub tick: i64,
    pub steamid: Option<u64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub health: Option<f64>,
    pub armor: Option<f64>,
    pub active_weapon_name: Option<String>,
    pub flash_duration: Option<f64>,
    pub balance: Option<f64>,
    pub current_equip_value: Option<f64>,
    pub has_defuser: bool,
    pub last_place_name: Option<String>,
    pub inventory: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DuelFrameRow {
    pub tick: i64,
    pub steamid: Option<u64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    pub health: Option<f64>,
    pub flash_duration: Option<f64>,
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
}

#[derive(Debug, Clone)]
pub struct FireVelocityRow {
    pub tick: i64,
    pub steamid: Option<u64>,
    pub vx: Option<f64>,
    pub vy: Option<f64>,
    pub vz: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GrenadeTrajectoryRaw {
    pub grenade: String,
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
    pub grenade: String,
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
    pub grenades: Vec<Vec<String>>,
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

fn ffill_bfill_round(vals: &[Option<f64>], scale: f64) -> Vec<i64> {
    let n = vals.len();
    let mut out = vec![0i64; n];
    let mut last: Option<f64> = None;
    for (i, v) in vals.iter().enumerate() {
        if let Some(f) = v.filter(|x| x.is_finite()) {
            last = Some(f);
            out[i] = round_ties_even_i64(f * scale);
        } else if let Some(f) = last {
            out[i] = round_ties_even_i64(f * scale);
        }
    }
    // bfill for leading None
    if let Some(first_valid_idx) = vals.iter().position(|v| v.is_some_and(|x| x.is_finite())) {
        let first_val = round_ties_even_i64(vals[first_valid_idx].unwrap() * scale);
        for item in out.iter_mut().take(first_valid_idx) {
            *item = first_val;
        }
    }
    out
}

pub fn build_replay(
    replay_rows: &[ReplayFrameRow],
    grenade_trajectories: &[GrenadeTrajectoryRaw],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    tickrate: i64,
    sample_rate: i64,
) -> Option<ReplayOutput> {
    if replay_rows.is_empty() {
        return None;
    }

    struct PrepReplayRow<'a> {
        tick: i64,
        pidx: usize,
        widx: i64,
        plidx: i64,
        grenades: Vec<String>,
        has_bomb: bool,
        src: &'a ReplayFrameRow,
    }

    let step = (tickrate / sample_rate.max(1)).max(1);
    let mut weapon_dict = StringDict::default();
    let mut place_dict = StringDict::default();

    let mut prep: Vec<PrepReplayRow<'_>> = Vec::with_capacity(replay_rows.len());
    for r in replay_rows {
        let Some(sid) = format_sid(r.steamid) else {
            continue;
        };
        let Some(pidx) = players.idx(Some(&sid)) else {
            continue;
        };
        let norm_w = normalize_weapon_name(r.active_weapon_name.as_deref());
        let widx = weapon_dict.index(norm_w.as_deref());
        let pl_trimmed = r
            .last_place_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let plidx = place_dict.index(pl_trimmed);
        let (_, _, _, grenades) = classify_inventory(&r.inventory);
        let has_bomb = r
            .inventory
            .iter()
            .any(|item| normalize_weapon_name(Some(item)).as_deref() == Some("c4"));
        prep.push(PrepReplayRow {
            tick: r.tick,
            pidx,
            widx,
            plidx,
            grenades,
            has_bomb,
            src: r,
        });
    }
    if prep.is_empty() {
        return None;
    }
    prep.sort_by_key(|r| r.tick);

    // projectile trajectories per round
    let mut proj_by_round: HashMap<i64, Vec<ReplayProjectile>> = HashMap::new();
    for tr in grenade_trajectories {
        let Some(rn) = round_model.round_for_tick(tr.start_tick) else {
            continue;
        };
        let thrower_index = players.idx(format_sid(tr.steamid).as_deref());
        proj_by_round.entry(rn).or_default().push(ReplayProjectile {
            grenade: tr.grenade.clone(),
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
        let left = prep.partition_point(|r| r.tick < grid[0]);
        let right = prep.partition_point(|r| r.tick <= *grid.last().unwrap());
        if right <= left {
            continue;
        }
        let slice = &prep[left..right];

        let mut by_player: BTreeMap<usize, HashMap<i64, &PrepReplayRow<'_>>> = BTreeMap::new();
        for row in slice {
            // keep="last" for duplicate ticks
            by_player.entry(row.pidx).or_default().insert(row.tick, row);
        }

        let mut player_tracks = Vec::new();
        for (pidx, tick_map) in by_player {
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
            let mut aligned_grenades: Vec<Option<&Vec<String>>> = vec![None; frame_count];

            let mut any_present = false;
            for (fi, &gt) in grid.iter().enumerate() {
                if let Some(r) = tick_map.get(&gt) {
                    any_present = true;
                    present[fi] = true;
                    raw_x[fi] = r.src.x;
                    raw_y[fi] = r.src.y;
                    raw_z[fi] = r.src.z;
                    raw_yaw[fi] = r.src.yaw;
                    raw_pitch[fi] = r.src.pitch;
                    raw_money[fi] = r.src.balance;
                    raw_equip[fi] = r.src.current_equip_value;
                    let h = round_ties_even_i64(r.src.health.unwrap_or(0.0)).clamp(0, 100);
                    hp[fi] = h;
                    armor[fi] = round_ties_even_i64(r.src.armor.unwrap_or(0.0)).clamp(0, 100);
                    flash[fi] = round_ties_even_i64(r.src.flash_duration.unwrap_or(0.0) * 10.0)
                        .clamp(0, 60);
                    widx[fi] = r.widx;
                    plidx[fi] = r.plidx;
                    aligned_grenades[fi] = Some(&r.grenades);
                    let alive = if h > 0 { 1 } else { 0 };
                    flags[fi] = alive * FLAG_ALIVE
                        + if r.has_bomb && alive == 1 {
                            FLAG_HAS_BOMB
                        } else {
                            0
                        }
                        + if r.src.has_defuser && alive == 1 {
                            FLAG_HAS_DEFUSE_KIT
                        } else {
                            0
                        };
                }
            }
            if !any_present {
                continue;
            }

            // ffill().bfill() grenades
            let mut grenades_out = vec![Vec::new(); frame_count];
            let mut last_g: Option<&Vec<String>> = None;
            let first_g = aligned_grenades.iter().position(|g| g.is_some());
            for fi in 0..frame_count {
                if let Some(g) = aligned_grenades[fi] {
                    last_g = Some(g);
                }
                let chosen = last_g.or_else(|| first_g.and_then(|idx| aligned_grenades[idx]));
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
        weapon_dict: weapon_dict.values,
        place_dict: place_dict.values,
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
    duel_rows: &[DuelFrameRow],
    duel_windows: &[(i64, i64)],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    tickrate: i64,
    kills_list: &[KillRow],
    damages_list: &[DamageRow],
    window_before_ms: i64,
    window_after_ms: i64,
) -> Option<DuelsOutput> {
    if duel_rows.is_empty() || duel_windows.is_empty() {
        return None;
    }

    struct PrepDuelRow<'a> {
        tick: i64,
        pidx: usize,
        src: &'a DuelFrameRow,
    }
    let mut prep: Vec<PrepDuelRow<'_>> = Vec::with_capacity(duel_rows.len());
    for r in duel_rows {
        let Some(sid) = format_sid(r.steamid) else {
            continue;
        };
        let Some(pidx) = players.idx(Some(&sid)) else {
            continue;
        };
        prep.push(PrepDuelRow {
            tick: r.tick,
            pidx,
            src: r,
        });
    }
    if prep.is_empty() {
        return None;
    }
    prep.sort_by_key(|r| r.tick);

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
    for &(start, end) in duel_windows {
        let Some(rn) = round_model.round_for_tick(start) else {
            continue;
        };
        if start > end {
            continue;
        }
        let frame_count = (end - start + 1) as usize;
        let left = prep.partition_point(|r| r.tick < start);
        let right = prep.partition_point(|r| r.tick <= end);
        if right <= left {
            continue;
        }
        let slice = &prep[left..right];

        let anchors: Vec<DuelAnchor> = anchors_all
            .iter()
            .filter(|a| start <= a.tick && a.tick <= end)
            .cloned()
            .collect();
        if anchors.is_empty() {
            continue;
        }

        let mut by_player: BTreeMap<usize, HashMap<i64, &PrepDuelRow<'_>>> = BTreeMap::new();
        for row in slice {
            by_player.entry(row.pidx).or_default().insert(row.tick, row);
        }

        let mut tracks = Vec::new();
        for (pidx, tick_map) in by_player {
            let mut raw_x = vec![None; frame_count];
            let mut raw_y = vec![None; frame_count];
            let mut raw_z = vec![None; frame_count];
            let mut raw_yaw = vec![None; frame_count];
            let mut raw_pitch = vec![None; frame_count];
            let mut hp = vec![0i64; frame_count];
            let mut flash = vec![0i64; frame_count];
            let mut any_present = false;

            for fi in 0..frame_count {
                let gt = start + fi as i64;
                if let Some(r) = tick_map.get(&gt) {
                    if r.src.x.is_some_and(|x| x.is_finite()) {
                        any_present = true;
                        raw_x[fi] = r.src.x;
                        raw_y[fi] = r.src.y;
                        raw_z[fi] = r.src.z;
                        raw_yaw[fi] = r.src.yaw;
                        raw_pitch[fi] = r.src.pitch;
                        hp[fi] = round_ties_even_i64(r.src.health.unwrap_or(0.0)).clamp(0, 100);
                        flash[fi] = round_ties_even_i64(r.src.flash_duration.unwrap_or(0.0) * 10.0)
                            .clamp(0, 60);
                    }
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
    fire_velocity: &[FireVelocityRow],
    players: &PlayerDirectory,
    round_model: &RoundModel,
) -> Option<ShotsOutput> {
    let mut vel_by_player: HashMap<usize, HashMap<i64, (i64, i64, i64)>> = HashMap::new();
    for r in fire_velocity {
        let Some(sid) = format_sid(r.steamid) else {
            continue;
        };
        let Some(pidx) = players.idx(Some(&sid)) else {
            continue;
        };
        let (Some(vx), Some(vy), Some(vz)) = (
            r.vx.filter(|v| v.is_finite()),
            r.vy.filter(|v| v.is_finite()),
            r.vz.filter(|v| v.is_finite()),
        ) else {
            continue;
        };
        vel_by_player.entry(pidx).or_default().insert(
            r.tick,
            (
                round_ties_even_i64(vx),
                round_ties_even_i64(vy),
                round_ties_even_i64(vz),
            ),
        );
    }

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
        let Some(idx) = players.idx(format_sid(r.user_steamid).as_deref()) else {
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
            .map(|r| weapon_dict.index(r.weapon.as_deref()))
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

        let player_vel = vel_by_player.get(&idx);
        let mut vx_vals = Vec::with_capacity(rows.len());
        let mut vy_vals = Vec::with_capacity(rows.len());
        let mut vz_vals = Vec::with_capacity(rows.len());
        for r in &rows {
            let (vx, vy, vz) = if let (Some(x), Some(y), Some(z)) = (
                r.velocity_x.filter(|v| v.is_finite()),
                r.velocity_y.filter(|v| v.is_finite()),
                r.velocity_z.filter(|v| v.is_finite()),
            ) {
                (
                    round_ties_even_i64(x),
                    round_ties_even_i64(y),
                    round_ties_even_i64(z),
                )
            } else {
                player_vel
                    .and_then(|m| m.get(&r.tick).copied())
                    .unwrap_or((0, 0, 0))
            };
            vx_vals.push(vx);
            vy_vals.push(vy);
            vz_vals.push(vz);
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
