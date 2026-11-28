use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

use ahash::AHashMap;
use anyhow::{bail, Context, Result};
use memmap2::MmapOptions;
use parser::first_pass::parser_settings::{rm_user_friendly_names, ParserInputs};
use parser::first_pass::prop_controller::{
    ENTITY_ID_ID, GRENADE_TYPE_ID, GRENADE_X, GRENADE_Y, GRENADE_Z, STEAMID_ID, TICK_ID,
};
use parser::maps::FRIENDLY_NAMES_MAPPING;
use parser::parse_demo::{DemoOutput, Parser, ParsingMode, HEADER_ENDS_AT_BYTE};
use parser::second_pass::game_events::GameEvent;
use parser::second_pass::parser_settings::create_huffman_lookup_table;
use parser::second_pass::variants::{PropColumn, VarVec, Variant};

use crate::enums::{BombEventKind, GrenadeType};
use crate::events::{
    round_f64, round_ties_even_i64, BlindRawRow, BombRawEvent, DeathRawRow, EconomyRawRow,
    GrenadeDetonationRaw, GrenadeExpireRaw, GrenadeThrowRaw, HurtRawRow,
};
use crate::rounds::{resolve_round_attempts, PlayerInfoRow, RoundMarkerRow, SideSampleRow};
use crate::streams::{DuelColumns, FireRawRow, GrenadeTrajectoryRaw, ReplayColumns};

static HUFFMAN_TABLE: OnceLock<Vec<(u8, u8)>> = OnceLock::new();

fn huffman() -> &'static Vec<(u8, u8)> {
    HUFFMAN_TABLE.get_or_init(create_huffman_lookup_table)
}

pub type ProgressFn<'a> = &'a dyn Fn(&str, f32);

#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub sample_rate: i64,
    pub research: bool,
    pub window_before_ms: i64,
    pub window_after_ms: i64,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            sample_rate: 8,
            research: false,
            window_before_ms: 2000,
            window_after_ms: 1000,
        }
    }
}

#[derive(Debug)]
pub struct RawDemo {
    pub timings: BTreeMap<String, f64>,
    pub header: HashMap<String, String>,
    pub tickrate: i64,
    pub sample_rate: i64,
    pub match_start_tick: i64,
    pub team_a_name: Option<String>,
    pub team_b_name: Option<String>,
    pub player_info: Vec<PlayerInfoRow>,
    pub round_starts: Vec<RoundMarkerRow>,
    pub round_freeze_ends: Vec<RoundMarkerRow>,
    pub round_ends: Vec<RoundMarkerRow>,
    pub deaths: Vec<DeathRawRow>,
    pub hurts: Vec<HurtRawRow>,
    pub fires: Vec<FireRawRow>,
    pub blinds: Vec<BlindRawRow>,
    pub bomb_events: Vec<BombRawEvent>,
    pub grenade_detonations: Vec<GrenadeDetonationRaw>,
    pub inferno_expires: Vec<GrenadeExpireRaw>,
    pub smoke_expires: Vec<GrenadeExpireRaw>,
    pub grenade_throws: Vec<GrenadeThrowRaw>,
    pub grenade_trajectories: Vec<GrenadeTrajectoryRaw>,
    pub replay_cols: ReplayColumns,
    pub replay_ticks: Vec<i64>,
    pub duel_cols: DuelColumns,
    pub duel_windows: Vec<(i64, i64)>,
    pub round_side_samples: Vec<SideSampleRow>,
    pub economy_raw: Vec<EconomyRawRow>,
    pub freeze_ticks: Vec<i64>,
}

fn build_real_name_map(props: &[String]) -> Result<(Vec<String>, AHashMap<String, String>)> {
    let real_names = rm_user_friendly_names(&props.to_vec())
        .map_err(|e| anyhow::anyhow!("failed to resolve property names: {e:?}"))?;
    let mut real_to_og = AHashMap::default();
    for (og, real) in props.iter().zip(real_names.iter()) {
        real_to_og.insert(real.clone(), og.clone());
    }
    for (k, v) in FRIENDLY_NAMES_MAPPING.entries() {
        real_to_og
            .entry((*v).to_string())
            .or_insert_with(|| (*k).to_string());
    }
    Ok((real_names, real_to_og))
}

#[allow(clippy::too_many_arguments)]
fn run_parser(
    mmap: &[u8],
    player_props: &[&str],
    other_props: &[&str],
    wanted_events: &[&str],
    wanted_ticks: &[i32],
    parse_ents: bool,
    parse_grenades: bool,
) -> Result<DemoOutput> {
    let mut p_props: Vec<String> = player_props.iter().map(|s| (*s).to_string()).collect();
    if parse_ents && !p_props.is_empty() {
        if !p_props.iter().any(|s| s == "tick") {
            p_props.push("tick".to_string());
        }
        if !p_props.iter().any(|s| s == "steamid") {
            p_props.push("steamid".to_string());
        }
    }
    let o_props: Vec<String> = other_props.iter().map(|s| (*s).to_string()).collect();
    let (real_player_props, mut real_to_og) = build_real_name_map(&p_props)?;
    let (real_other_props, other_to_og) = build_real_name_map(&o_props)?;
    for (k, v) in other_to_og {
        real_to_og.insert(k, v);
    }

    let inputs = ParserInputs {
        real_name_to_og_name: real_to_og,
        wanted_players: vec![],
        wanted_player_props: real_player_props,
        wanted_other_props: real_other_props,
        wanted_prop_states: AHashMap::default(),
        wanted_ticks: wanted_ticks.to_vec(),
        wanted_events: wanted_events.iter().map(|s| (*s).to_string()).collect(),
        parse_ents,
        parse_projectiles: parse_grenades,
        parse_grenades,
        parse_weapons: false,
        vector_props_as_lists: false,
        only_header: false,
        only_convars: false,
        huffman_lookup_table: huffman(),
        order_by_steamid: false,
        list_props: false,
        fallback_bytes: None,
    };
    let mut p = Parser::new(inputs, ParsingMode::Normal);
    p.parse_demo(mmap)
        .map_err(|e| anyhow::anyhow!("demoparser error: {e}"))
}

fn var_i64(v: Option<&Variant>) -> Option<i64> {
    match v? {
        Variant::I32(x) => Some(*x as i64),
        Variant::U32(x) => Some(*x as i64),
        Variant::U64(x) => Some(*x as i64),
        Variant::F32(x) if x.is_finite() => Some(*x as i64),
        Variant::Bool(b) => Some(if *b { 1 } else { 0 }),
        Variant::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn var_u64(v: Option<&Variant>) -> Option<u64> {
    match v? {
        Variant::U64(x) => Some(*x),
        Variant::I32(x) if *x >= 0 => Some(*x as u64),
        Variant::U32(x) => Some(*x as u64),
        Variant::F32(x) if x.is_finite() && *x >= 0.0 => Some(*x as u64),
        Variant::String(s) => {
            let t = s.trim();
            let t = t.strip_suffix(".0").unwrap_or(t);
            t.parse::<u64>().ok()
        }
        _ => None,
    }
}

fn var_f64(v: Option<&Variant>) -> Option<f64> {
    match v? {
        Variant::F32(x) if x.is_finite() => Some(*x as f64),
        Variant::I32(x) => Some(*x as f64),
        Variant::U32(x) => Some(*x as f64),
        Variant::U64(x) => Some(*x as f64),
        Variant::String(s) => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

fn var_bool(v: Option<&Variant>) -> bool {
    match v {
        Some(Variant::Bool(b)) => *b,
        Some(Variant::I32(x)) => *x != 0,
        Some(Variant::U32(x)) => *x != 0,
        Some(Variant::U64(x)) => *x != 0,
        Some(Variant::F32(x)) => x.is_finite() && *x != 0.0,
        Some(Variant::String(s)) => {
            matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true")
        }
        _ => false,
    }
}

fn var_str(v: Option<&Variant>) -> Option<String> {
    match v? {
        Variant::String(s) => Some(s.clone()),
        Variant::I32(x) => Some(x.to_string()),
        Variant::U32(x) => Some(x.to_string()),
        Variant::U64(x) => Some(x.to_string()),
        Variant::F32(x) if x.is_finite() => Some(x.to_string()),
        _ => None,
    }
}

fn event_field<'a>(ev: &'a GameEvent, name: &str) -> Option<&'a Variant> {
    ev.fields
        .iter()
        .find(|f| f.name == name)
        .and_then(|f| f.data.as_ref())
}

fn event_field_ci<'a>(ev: &'a GameEvent, name: &str) -> Option<&'a Variant> {
    event_field(ev, name).or_else(|| {
        let lower = name.to_ascii_lowercase();
        ev.fields
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&lower))
            .and_then(|f| f.data.as_ref())
    })
}

struct ColReader<'a> {
    cols: HashMap<String, &'a PropColumn>,
    tick_col: Option<&'a PropColumn>,
    steamid_col: Option<&'a PropColumn>,
    len: usize,
}

impl<'a> ColReader<'a> {
    fn from_output(out: &'a DemoOutput) -> Self {
        let mut cols = HashMap::new();
        for info in &out.prop_controller.prop_infos {
            if let Some(col) = out.df.get(&info.id) {
                cols.insert(info.prop_friendly_name.clone(), col);
                cols.insert(info.prop_name.clone(), col);
            }
        }
        let tick_col = out.df.get(&TICK_ID).or_else(|| cols.get("tick").copied());
        let steamid_col = out
            .df
            .get(&STEAMID_ID)
            .or_else(|| cols.get("steamid").copied());
        let len = tick_col
            .or(steamid_col)
            .or_else(|| cols.values().next().copied())
            .map(col_len)
            .unwrap_or(0);
        Self {
            cols,
            tick_col,
            steamid_col,
            len,
        }
    }

    fn get(&self, name: &str) -> Option<&'a PropColumn> {
        self.cols.get(name).copied()
    }
}

fn col_len(col: &PropColumn) -> usize {
    match &col.data {
        Some(VarVec::U32(v)) => v.len(),
        Some(VarVec::Bool(v)) => v.len(),
        Some(VarVec::U64(v)) => v.len(),
        Some(VarVec::F32(v)) => v.len(),
        Some(VarVec::I32(v)) => v.len(),
        Some(VarVec::String(v)) => v.len(),
        Some(VarVec::StringVec(v)) => v.len(),
        Some(VarVec::U64Vec(v)) => v.len(),
        Some(VarVec::U32Vec(v)) => v.len(),
        Some(VarVec::XYVec(v)) => v.len(),
        Some(VarVec::XYZVec(v)) => v.len(),
        Some(VarVec::StrDict(d)) => d.codes.len(),
        Some(VarVec::Stickers(v)) => v.len(),
        Some(VarVec::InputHistory(v)) => v.len(),
        Some(VarVec::UserCmdSubtickMoves(v)) => v.len(),
        Some(VarVec::List(v)) => v.len(),
        Some(VarVec::Bytes(v)) => v.len(),
        None => 0,
    }
}

fn col_i64(col: Option<&PropColumn>, i: usize) -> Option<i64> {
    match col?.data.as_ref()? {
        VarVec::I32(v) => v.get(i).and_then(|x| *x).map(|x| x as i64),
        VarVec::U32(v) => v.get(i).and_then(|x| *x).map(|x| x as i64),
        VarVec::U64(v) => v.get(i).and_then(|x| *x).map(|x| x as i64),
        VarVec::F32(v) => v
            .get(i)
            .and_then(|x| *x)
            .filter(|x| x.is_finite())
            .map(|x| x as i64),
        VarVec::Bool(v) => v.get(i).and_then(|x| *x).map(|b| if b { 1 } else { 0 }),
        _ => None,
    }
}

fn col_u64(col: Option<&PropColumn>, i: usize) -> Option<u64> {
    match col?.data.as_ref()? {
        VarVec::U64(v) => v.get(i).and_then(|x| *x),
        VarVec::I32(v) => v
            .get(i)
            .and_then(|x| *x)
            .filter(|x| *x >= 0)
            .map(|x| x as u64),
        VarVec::U32(v) => v.get(i).and_then(|x| *x).map(|x| x as u64),
        VarVec::F32(v) => v
            .get(i)
            .and_then(|x| *x)
            .filter(|x| x.is_finite() && *x >= 0.0)
            .map(|x| x as u64),
        VarVec::String(v) => v
            .get(i)
            .and_then(|x| x.as_deref())
            .and_then(|s| s.parse().ok()),
        VarVec::StrDict(d) => d.get(i).and_then(|s| s.parse().ok()),
        _ => None,
    }
}

fn col_f32(col: Option<&PropColumn>, i: usize) -> Option<f32> {
    match col?.data.as_ref()? {
        VarVec::F32(v) => v.get(i).and_then(|x| *x).filter(|x| x.is_finite()),
        VarVec::I32(v) => v.get(i).and_then(|x| *x).map(|x| x as f32),
        VarVec::U32(v) => v.get(i).and_then(|x| *x).map(|x| x as f32),
        VarVec::U64(v) => v.get(i).and_then(|x| *x).map(|x| x as f32),
        _ => None,
    }
}

fn col_f64(col: Option<&PropColumn>, i: usize) -> Option<f64> {
    match col?.data.as_ref()? {
        VarVec::F32(v) => v
            .get(i)
            .and_then(|x| *x)
            .filter(|x| x.is_finite())
            .map(|x| x as f64),
        VarVec::I32(v) => v.get(i).and_then(|x| *x).map(|x| x as f64),
        VarVec::U32(v) => v.get(i).and_then(|x| *x).map(|x| x as f64),
        VarVec::U64(v) => v.get(i).and_then(|x| *x).map(|x| x as f64),
        _ => None,
    }
}

fn col_bool(col: Option<&PropColumn>, i: usize) -> bool {
    match col.and_then(|c| c.data.as_ref()) {
        Some(VarVec::Bool(v)) => v.get(i).and_then(|x| *x).unwrap_or(false),
        Some(VarVec::I32(v)) => v.get(i).and_then(|x| *x).unwrap_or(0) != 0,
        Some(VarVec::U32(v)) => v.get(i).and_then(|x| *x).unwrap_or(0) != 0,
        Some(VarVec::F32(v)) => v
            .get(i)
            .and_then(|x| *x)
            .is_some_and(|x| x.is_finite() && x != 0.0),
        _ => false,
    }
}

fn col_str_ref(col: Option<&PropColumn>, i: usize) -> Option<&str> {
    match col?.data.as_ref()? {
        VarVec::String(v) => v.get(i).and_then(|x| x.as_deref()),
        VarVec::StrDict(d) => d.get(i),
        _ => None,
    }
}

fn col_str(col: Option<&PropColumn>, i: usize) -> Option<String> {
    match col?.data.as_ref()? {
        VarVec::String(v) => v.get(i).and_then(|x| x.clone()),
        VarVec::StrDict(d) => d.get(i).map(str::to_string),
        VarVec::I32(v) => v.get(i).and_then(|x| *x).map(|x| x.to_string()),
        VarVec::U32(v) => v.get(i).and_then(|x| *x).map(|x| x.to_string()),
        _ => None,
    }
}

fn col_str_vec_ref(col: Option<&PropColumn>, i: usize) -> &[String] {
    match col.and_then(|c| c.data.as_ref()) {
        Some(VarVec::StringVec(v)) => v.get(i).map(Vec::as_slice).unwrap_or(&[]),
        _ => &[],
    }
}

fn round_spans(
    round_ends: &[RoundMarkerRow],
    round_freeze_ends: &[RoundMarkerRow],
    round_starts: &[RoundMarkerRow],
) -> Vec<(i64, i64)> {
    let attempts = resolve_round_attempts(round_starts, round_freeze_ends, round_ends);
    let len = attempts.len();
    attempts
        .into_iter()
        .enumerate()
        .filter(|(_, (w, _, _))| w.freeze_end_tick > 0)
        .map(|(idx, (w, _, _))| {
            let end_t = match w.next_start_tick {
                Some(ns) if idx + 1 < len => ns - 1,
                _ => w.end_tick,
            };
            (w.freeze_end_tick, end_t)
        })
        .collect()
}

pub fn build_sample_ticks(
    round_ends: &[RoundMarkerRow],
    round_freeze_ends: &[RoundMarkerRow],
    round_starts: &[RoundMarkerRow],
    step: i64,
) -> Vec<i64> {
    let step = step.max(1);
    let mut set = BTreeSet::new();
    for (start_t, end_t) in round_spans(round_ends, round_freeze_ends, round_starts) {
        let mut t = start_t;
        while t <= end_t {
            set.insert(t);
            t += step;
        }
    }
    set.into_iter().collect()
}

pub fn merge_windows(
    anchor_ticks: &[i64],
    round_ends: &[RoundMarkerRow],
    round_freeze_ends: &[RoundMarkerRow],
    round_starts: &[RoundMarkerRow],
    tickrate: i64,
    before_ms: i64,
    after_ms: i64,
) -> Vec<(i64, i64)> {
    let mut spans = round_spans(round_ends, round_freeze_ends, round_starts);
    if spans.is_empty() || anchor_ticks.is_empty() {
        return Vec::new();
    }
    let before = (before_ms * tickrate) / 1000;
    let after = (after_ms * tickrate) / 1000;
    spans.sort_unstable();
    let anchors: BTreeSet<i64> = anchor_ticks.iter().copied().filter(|&t| t > 0).collect();

    let mut windows = Vec::new();
    for t in anchors {
        let idx = spans.partition_point(|&(s, _)| s <= t);
        if idx == 0 {
            continue;
        }
        let (lo, hi) = spans[idx - 1];
        if t < lo || t > hi {
            continue;
        }
        windows.push((lo.max(t - before), hi.min(t + after)));
    }
    if windows.is_empty() {
        return Vec::new();
    }
    windows.sort_unstable();
    let mut merged = vec![windows[0]];
    for (s, e) in windows.into_iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if s <= last.1 + 1 {
            last.1 = last.1.max(e);
        } else {
            merged.push((s, e));
        }
    }
    merged
}

fn to_round_marker(ev: &GameEvent) -> RoundMarkerRow {
    let reason_v = event_field(ev, "reason");
    RoundMarkerRow {
        tick: ev.tick as i64,
        total_rounds_played: var_i64(event_field(ev, "total_rounds_played")).unwrap_or(0),
        winner: var_str(event_field(ev, "winner")),
        reason_str: var_str(reason_v),
        reason_num: var_i64(reason_v),
    }
}

fn to_bomb_row(ev: &GameEvent, kind: BombEventKind) -> BombRawEvent {
    BombRawEvent {
        kind,
        tick: ev.tick as i64,
        total_rounds_played: var_i64(event_field(ev, "total_rounds_played")).unwrap_or(0),
        user_steamid: var_u64(
            event_field(ev, "user_steamid").or_else(|| event_field(ev, "steamid")),
        ),
        user_x: var_f64(event_field_ci(ev, "user_X")),
        user_y: var_f64(event_field_ci(ev, "user_Y")),
        user_z: var_f64(event_field_ci(ev, "user_Z")),
        user_last_place_name: var_str(event_field(ev, "user_last_place_name")),
    }
}

fn event_steamid(ev: &GameEvent) -> Option<u64> {
    var_u64(
        event_field(ev, "user_steamid")
            .or_else(|| event_field(ev, "steamid"))
            .or_else(|| event_field(ev, "attacker_steamid")),
    )
}

fn event_entity_id(ev: &GameEvent) -> Option<i64> {
    var_i64(
        event_field(ev, "entityid")
            .or_else(|| event_field(ev, "entity_id"))
            .or_else(|| event_field(ev, "grenade_entity_id")),
    )
}

fn rebuild_blinds(mmap: &[u8], detonates: &[GameEvent]) -> Result<Vec<BlindRawRow>> {
    let mut det_by_tick: BTreeMap<i64, Vec<&GameEvent>> = BTreeMap::new();
    for d in detonates {
        let tick = d.tick as i64;
        if tick > 0 {
            det_by_tick.entry(tick).or_default().push(d);
        }
    }
    if det_by_tick.is_empty() {
        return Ok(Vec::new());
    }
    let mut sample_ticks = BTreeSet::new();
    for &t in det_by_tick.keys() {
        sample_ticks.insert((t - 1) as i32);
        sample_ticks.insert((t + 2) as i32);
    }
    let ticks_vec: Vec<i32> = sample_ticks.into_iter().collect();
    let out = run_parser(
        mmap,
        &["flash_duration", "X", "Y"],
        &[],
        &[],
        &ticks_vec,
        true,
        false,
    )?;
    let reader = ColReader::from_output(&out);
    let Some(fd_col) = reader.get("flash_duration") else {
        return Ok(Vec::new());
    };
    let x_col = reader.get("X");
    let y_col = reader.get("Y");

    type BlindStateMap = HashMap<(i64, u64), (f64, Option<f64>, Option<f64>)>;
    let mut state: BlindStateMap = HashMap::new();
    let mut sids = BTreeSet::new();
    for i in 0..reader.len {
        let Some(sid) = col_u64(reader.steamid_col, i) else {
            continue;
        };
        let Some(tick) = col_i64(reader.tick_col, i) else {
            continue;
        };
        let fd = col_f64(Some(fd_col), i).unwrap_or(0.0);
        let x = col_f64(x_col, i);
        let y = col_f64(y_col, i);
        sids.insert(sid);
        state.insert((tick, sid), (fd, x, y));
    }

    let mut result = Vec::new();
    for (&tick, dets) in &det_by_tick {
        for &sid in &sids {
            let before = state.get(&(tick - 1, sid)).map(|s| s.0).unwrap_or(0.0);
            let Some(&(after, px, py)) = state.get(&(tick + 2, sid)) else {
                continue;
            };
            if after <= before + 0.1 {
                continue;
            }
            let mut det = dets[0];
            if dets.len() > 1 {
                if let (Some(pxv), Some(pyv)) = (px, py) {
                    det = dets
                        .iter()
                        .copied()
                        .min_by(|a, b| {
                            let ax = var_f64(event_field_ci(a, "x")).unwrap_or(0.0);
                            let ay = var_f64(event_field_ci(a, "y")).unwrap_or(0.0);
                            let bx = var_f64(event_field_ci(b, "x")).unwrap_or(0.0);
                            let by = var_f64(event_field_ci(b, "y")).unwrap_or(0.0);
                            let da = (ax - pxv).powi(2) + (ay - pyv).powi(2);
                            let db = (bx - pxv).powi(2) + (by - pyv).powi(2);
                            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .unwrap_or(dets[0]);
                }
            }
            result.push(BlindRawRow {
                tick,
                total_rounds_played: var_i64(event_field(det, "total_rounds_played")).unwrap_or(0),
                attacker_steamid: var_u64(event_field(det, "user_steamid")),
                user_steamid: Some(sid),
                blind_duration: after,
            });
        }
    }
    Ok(result)
}

fn extract_grenade_paths(
    mmap: &[u8],
    sample_ticks: &[i64],
) -> Result<(Vec<GrenadeThrowRaw>, Vec<GrenadeTrajectoryRaw>)> {
    let out = run_parser(mmap, &[], &[], &[], &[], true, true)?;
    struct ProjPt {
        eid: i64,
        tick: i64,
        x: f64,
        y: f64,
        z: f64,
        gtype: GrenadeType,
        steamid: Option<u64>,
    }
    let gt_col = out.df.get(&GRENADE_TYPE_ID);
    let eid_col = out.df.get(&ENTITY_ID_ID);
    let tick_col = out.df.get(&TICK_ID);
    let sid_col = out.df.get(&STEAMID_ID);
    let x_col = out.df.get(&GRENADE_X);
    let y_col = out.df.get(&GRENADE_Y);
    let z_col = out.df.get(&GRENADE_Z);
    let n_rows = tick_col.map(col_len).unwrap_or(0);

    let mut pts: Vec<ProjPt> = Vec::with_capacity(n_rows);
    for i in 0..n_rows {
        let Some(gt) = col_str_ref(gt_col, i) else {
            continue;
        };
        if !gt.ends_with("Projectile") {
            continue;
        }
        let Some(gtype) = GrenadeType::from_projectile_class(gt) else {
            continue;
        };
        let (Some(x), Some(y), Some(z), Some(tick), Some(eid)) = (
            col_f64(x_col, i),
            col_f64(y_col, i),
            col_f64(z_col, i),
            col_i64(tick_col, i),
            col_i64(eid_col, i),
        ) else {
            continue;
        };
        pts.push(ProjPt {
            eid,
            tick,
            x,
            y,
            z,
            gtype,
            steamid: col_u64(sid_col, i),
        });
    }
    if pts.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    pts.sort_by_key(|p| (p.eid, p.tick));

    let mut segs: Vec<&[ProjPt]> = Vec::new();
    let mut seg_start = 0usize;
    for i in 1..pts.len() {
        if pts[i].eid != pts[i - 1].eid || (pts[i].tick - pts[i - 1].tick) > 64 {
            segs.push(&pts[seg_start..i]);
            seg_start = i;
        }
    }
    segs.push(&pts[seg_start..]);

    let grid: Vec<i64> = {
        let s: BTreeSet<i64> = sample_ticks.iter().copied().collect();
        s.into_iter().collect()
    };

    let mut throws = Vec::new();
    let mut trajectories = Vec::new();
    for seg in segs {
        let first = &seg[0];
        let throw_tick = first.tick;
        let last_tick = seg.last().unwrap().tick;
        throws.push(GrenadeThrowRaw {
            grenade_entity_id: Some(first.eid),
            grenade: first.gtype,
            tick: throw_tick,
            destroy_tick: last_tick,
            steamid: first.steamid,
            x: first.x,
            y: first.y,
            z: first.z,
        });

        let lo = grid.partition_point(|&t| t < throw_tick);
        let hi = grid.partition_point(|&t| t <= last_tick);
        let mut gticks: Vec<i64> = grid[lo..hi].to_vec();
        if gticks.is_empty() {
            gticks.push(throw_tick);
        }

        let mut xs = Vec::with_capacity(gticks.len());
        let mut ys = Vec::with_capacity(gticks.len());
        let mut zs = Vec::with_capacity(gticks.len());
        for &t in &gticks {
            let idx = seg.partition_point(|p| p.tick < t);
            let chosen = if idx == 0 {
                &seg[0]
            } else if idx >= seg.len() {
                &seg[seg.len() - 1]
            } else {
                let d_prev = (t - seg[idx - 1].tick).abs();
                let d_next = (seg[idx].tick - t).abs();
                if d_prev <= d_next {
                    &seg[idx - 1]
                } else {
                    &seg[idx]
                }
            };
            xs.push(round_ties_even_i64(chosen.x));
            ys.push(round_ties_even_i64(chosen.y));
            zs.push(round_ties_even_i64(chosen.z));
        }

        while xs.len() >= 2 {
            let n = xs.len();
            let dx = xs[n - 1] - xs[n - 2];
            let dy = ys[n - 1] - ys[n - 2];
            let dz = zs[n - 1] - zs[n - 2];
            if dx * dx + dy * dy + dz * dz <= 100 {
                xs.pop();
                ys.pop();
                zs.pop();
            } else {
                break;
            }
        }

        trajectories.push(GrenadeTrajectoryRaw {
            grenade: first.gtype,
            steamid: first.steamid,
            start_tick: gticks[0],
            xs,
            ys,
            zs,
        });
    }

    Ok((throws, trajectories))
}

pub fn parse_demo(
    dem_path: &Path,
    opts: &ParseOptions,
    progress: Option<ProgressFn<'_>>,
) -> Result<RawDemo> {
    let mut timings: BTreeMap<String, f64> = BTreeMap::new();
    let parse_started = Instant::now();
    let record = |timings: &mut BTreeMap<String, f64>, stage: &str, started: Instant| {
        timings.insert(
            stage.to_string(),
            round_f64(started.elapsed().as_secs_f64(), 3),
        );
    };
    let report = |stage: &str, frac: f32| {
        if let Some(cb) = progress {
            cb(stage, frac);
        }
    };

    report("open demo", 0.01);
    let st = Instant::now();
    let file = File::open(dem_path)
        .with_context(|| format!("failed to open demo: {}", dem_path.display()))?;
    let mmap = unsafe {
        MmapOptions::new()
            .map(&file)
            .with_context(|| format!("failed to mmap demo: {}", dem_path.display()))?
    };
    if mmap.len() < HEADER_ENDS_AT_BYTE {
        bail!(
            "truncated or invalid demo file (< {HEADER_ENDS_AT_BYTE} bytes): {}",
            dem_path.display()
        );
    }
    record(&mut timings, "parse.openDemo", st);

    // Pass 1: consolidated standard events + header
    report("events", 0.05);
    let st = Instant::now();
    let ev_out = run_parser(
        &mmap,
        &["steamid", "X", "Y", "Z", "active_weapon", "last_place_name"],
        &["total_rounds_played"],
        &[
            "round_start",
            "round_freeze_end",
            "round_end",
            "player_blind",
            "round_announce_match_start",
            "player_death",
            "player_hurt",
            "bomb_planted",
            "bomb_defused",
            "bomb_exploded",
            "bomb_beginplant",
            "bomb_begindefuse",
            "bomb_dropped",
            "bomb_pickup",
            "smokegrenade_detonate",
            "flashbang_detonate",
            "hegrenade_detonate",
            "inferno_startburn",
            "decoy_detonate",
            "inferno_expire",
            "smokegrenade_expired",
        ],
        &[],
        true,
        false,
    )?;

    let header: HashMap<String, String> = ev_out
        .header
        .map(|h| h.into_iter().collect())
        .unwrap_or_default();
    let tickrate = header
        .get("tick_rate")
        .and_then(|s| s.parse::<f64>().ok())
        .map(|v| v as i64)
        .filter(|&v| v > 0)
        .unwrap_or(64);

    let mut round_starts = Vec::new();
    let mut round_freeze_ends = Vec::new();
    let mut round_ends = Vec::new();
    let mut blinds = Vec::new();
    let mut match_start_ticks = Vec::new();
    let mut deaths = Vec::new();
    let mut hurts = Vec::new();
    let mut bomb_events = Vec::new();
    let mut grenade_detonations = Vec::new();
    let mut inferno_expires = Vec::new();
    let mut smoke_expires = Vec::new();
    let mut flashbang_detonate_events = Vec::new();

    for ev in ev_out.game_events {
        match ev.name.as_str() {
            "round_start" => round_starts.push(to_round_marker(&ev)),
            "round_freeze_end" => round_freeze_ends.push(to_round_marker(&ev)),
            "round_end" => round_ends.push(to_round_marker(&ev)),
            "round_announce_match_start" => match_start_ticks.push(ev.tick as i64),
            "player_blind" => {
                blinds.push(BlindRawRow {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    attacker_steamid: var_u64(event_field(&ev, "attacker_steamid")),
                    user_steamid: var_u64(event_field(&ev, "user_steamid")),
                    blind_duration: var_f64(
                        event_field(&ev, "blind_duration").or_else(|| event_field(&ev, "duration")),
                    )
                    .unwrap_or(0.0),
                });
            }
            "player_death" => {
                deaths.push(DeathRawRow {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    user_steamid: var_u64(event_field(&ev, "user_steamid")),
                    attacker_steamid: var_u64(event_field(&ev, "attacker_steamid")),
                    assister_steamid: var_u64(event_field(&ev, "assister_steamid")),
                    weapon: var_str(event_field(&ev, "weapon")),
                    attacker_active_weapon: var_str(event_field(&ev, "attacker_active_weapon")),
                    user_active_weapon: var_str(event_field(&ev, "user_active_weapon")),
                    headshot: var_bool(event_field(&ev, "headshot")),
                    assistedflash: var_bool(event_field(&ev, "assistedflash")),
                    thrusmoke: var_bool(event_field(&ev, "thrusmoke")),
                    noscope: var_bool(event_field(&ev, "noscope")),
                    penetrated: var_i64(
                        event_field(&ev, "penetrated_objects")
                            .or_else(|| event_field(&ev, "penetrated")),
                    )
                    .unwrap_or(0),
                    attacker_x: var_f64(event_field_ci(&ev, "attacker_X")),
                    attacker_y: var_f64(event_field_ci(&ev, "attacker_Y")),
                    attacker_z: var_f64(event_field_ci(&ev, "attacker_Z")),
                    user_x: var_f64(event_field_ci(&ev, "user_X")),
                    user_y: var_f64(event_field_ci(&ev, "user_Y")),
                    user_z: var_f64(event_field_ci(&ev, "user_Z")),
                });
            }
            "player_hurt" => {
                hurts.push(HurtRawRow {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    user_steamid: var_u64(event_field(&ev, "user_steamid")),
                    attacker_steamid: var_u64(event_field(&ev, "attacker_steamid")),
                    weapon: var_str(event_field(&ev, "weapon")),
                    hitgroup: var_str(event_field(&ev, "hitgroup")),
                    dmg_health: var_i64(event_field(&ev, "dmg_health")).unwrap_or(0),
                    dmg_armor: var_i64(event_field(&ev, "dmg_armor")).unwrap_or(0),
                    armor: var_i64(event_field(&ev, "armor")).unwrap_or(0),
                    attacker_x: var_f64(event_field_ci(&ev, "attacker_X")),
                    attacker_y: var_f64(event_field_ci(&ev, "attacker_Y")),
                    attacker_z: var_f64(event_field_ci(&ev, "attacker_Z")),
                    user_x: var_f64(event_field_ci(&ev, "user_X")),
                    user_y: var_f64(event_field_ci(&ev, "user_Y")),
                    user_z: var_f64(event_field_ci(&ev, "user_Z")),
                });
            }
            "bomb_planted" => bomb_events.push(to_bomb_row(&ev, BombEventKind::Planted)),
            "bomb_defused" => bomb_events.push(to_bomb_row(&ev, BombEventKind::Defused)),
            "bomb_exploded" => bomb_events.push(to_bomb_row(&ev, BombEventKind::Exploded)),
            "bomb_beginplant" => bomb_events.push(to_bomb_row(&ev, BombEventKind::PlantBegin)),
            "bomb_begindefuse" => bomb_events.push(to_bomb_row(&ev, BombEventKind::DefuseBegin)),
            "bomb_dropped" => bomb_events.push(to_bomb_row(&ev, BombEventKind::Dropped)),
            "bomb_pickup" => bomb_events.push(to_bomb_row(&ev, BombEventKind::PickedUp)),
            "smokegrenade_detonate" => {
                grenade_detonations.push(GrenadeDetonationRaw {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    grenade_type: GrenadeType::Smoke,
                    user_steamid: event_steamid(&ev),
                    entity_id: event_entity_id(&ev),
                    x: var_f64(event_field_ci(&ev, "x")),
                    y: var_f64(event_field_ci(&ev, "y")),
                    z: var_f64(event_field_ci(&ev, "z")),
                });
            }
            "flashbang_detonate" => {
                grenade_detonations.push(GrenadeDetonationRaw {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    grenade_type: GrenadeType::Flashbang,
                    user_steamid: event_steamid(&ev),
                    entity_id: event_entity_id(&ev),
                    x: var_f64(event_field_ci(&ev, "x")),
                    y: var_f64(event_field_ci(&ev, "y")),
                    z: var_f64(event_field_ci(&ev, "z")),
                });
                flashbang_detonate_events.push(ev);
            }
            "hegrenade_detonate" => {
                grenade_detonations.push(GrenadeDetonationRaw {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    grenade_type: GrenadeType::Hegrenade,
                    user_steamid: event_steamid(&ev),
                    entity_id: event_entity_id(&ev),
                    x: var_f64(event_field_ci(&ev, "x")),
                    y: var_f64(event_field_ci(&ev, "y")),
                    z: var_f64(event_field_ci(&ev, "z")),
                });
            }
            "inferno_startburn" => {
                grenade_detonations.push(GrenadeDetonationRaw {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    grenade_type: GrenadeType::Molotov,
                    user_steamid: event_steamid(&ev),
                    entity_id: event_entity_id(&ev),
                    x: var_f64(event_field_ci(&ev, "x")),
                    y: var_f64(event_field_ci(&ev, "y")),
                    z: var_f64(event_field_ci(&ev, "z")),
                });
            }
            "decoy_detonate" => {
                grenade_detonations.push(GrenadeDetonationRaw {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    grenade_type: GrenadeType::Decoy,
                    user_steamid: event_steamid(&ev),
                    entity_id: event_entity_id(&ev),
                    x: var_f64(event_field_ci(&ev, "x")),
                    y: var_f64(event_field_ci(&ev, "y")),
                    z: var_f64(event_field_ci(&ev, "z")),
                });
            }
            "inferno_expire" => {
                inferno_expires.push(GrenadeExpireRaw {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    entity_id: event_entity_id(&ev),
                    x: var_f64(event_field_ci(&ev, "x")),
                    y: var_f64(event_field_ci(&ev, "y")),
                    z: var_f64(event_field_ci(&ev, "z")),
                });
            }
            "smokegrenade_expired" => {
                smoke_expires.push(GrenadeExpireRaw {
                    tick: ev.tick as i64,
                    total_rounds_played: var_i64(event_field(&ev, "total_rounds_played"))
                        .unwrap_or(0),
                    entity_id: event_entity_id(&ev),
                    x: var_f64(event_field_ci(&ev, "x")),
                    y: var_f64(event_field_ci(&ev, "y")),
                    z: var_f64(event_field_ci(&ev, "z")),
                });
            }
            _ => {}
        }
    }
    record(&mut timings, "parse.events", st);

    // Pass 2 (conditional): rebuild blinds reusing already-captured flashbang_detonate events
    if blinds.is_empty() && !flashbang_detonate_events.is_empty() {
        let st = Instant::now();
        blinds = rebuild_blinds(&mmap, &flashbang_detonate_events)?;
        record(&mut timings, "parse.blindRebuild", st);
    }
    drop(flashbang_detonate_events);

    // Pass 3: weapon_fire isolated with velocity_X/Y/Z
    report("shots", 0.25);
    let st = Instant::now();
    let fires_out = run_parser(
        &mmap,
        &[
            "X",
            "Y",
            "Z",
            "yaw",
            "pitch",
            "velocity_X",
            "velocity_Y",
            "velocity_Z",
        ],
        &["total_rounds_played"],
        &["weapon_fire"],
        &[],
        true,
        false,
    )?;
    let fires: Vec<FireRawRow> = fires_out
        .game_events
        .into_iter()
        .filter(|ev| ev.name == "weapon_fire")
        .map(|ev| FireRawRow {
            tick: ev.tick as i64,
            total_rounds_played: var_i64(event_field(&ev, "total_rounds_played")).unwrap_or(0),
            user_steamid: var_u64(
                event_field(&ev, "user_steamid").or_else(|| event_field(&ev, "steamid")),
            ),
            weapon: var_str(event_field(&ev, "weapon")),
            user_x: var_f64(event_field_ci(&ev, "user_X")),
            user_y: var_f64(event_field_ci(&ev, "user_Y")),
            user_z: var_f64(event_field_ci(&ev, "user_Z")),
            user_yaw: var_f64(event_field(&ev, "user_yaw")),
            user_pitch: var_f64(event_field(&ev, "user_pitch")),
            velocity_x: var_f64(event_field_ci(&ev, "user_velocity_X")),
            velocity_y: var_f64(event_field_ci(&ev, "user_velocity_Y")),
            velocity_z: var_f64(event_field_ci(&ev, "user_velocity_Z")),
        })
        .collect();
    record(&mut timings, "parse.shots", st);

    // Pass 4: merged sparse metadata + side samples + economy ticks
    report("metadata & economy", 0.40);
    let st = Instant::now();
    let match_start_tick = match_start_ticks
        .first()
        .copied()
        .or_else(|| round_freeze_ends.first().map(|r| r.tick))
        .unwrap_or(1);

    let round_side_tick_set: HashSet<i64> = round_freeze_ends
        .iter()
        .filter(|r| r.tick > 0)
        .map(|r| r.tick + 16)
        .collect();
    let freeze_ticks: Vec<i64> = round_freeze_ends
        .iter()
        .filter(|r| r.tick > 0)
        .map(|r| r.tick)
        .collect::<BTreeSet<i64>>()
        .into_iter()
        .collect();
    let freeze_tick_set: HashSet<i64> = freeze_ticks.iter().copied().collect();

    let mut meta_ticks_set = BTreeSet::new();
    meta_ticks_set.insert(match_start_tick as i32);
    for &t in &round_side_tick_set {
        meta_ticks_set.insert(t as i32);
    }
    for &t in &freeze_ticks {
        meta_ticks_set.insert(t as i32);
    }
    let meta_ticks: Vec<i32> = meta_ticks_set.into_iter().collect();

    let mut team_a_name: Option<String> = None;
    let mut team_b_name: Option<String> = None;
    let mut player_info = Vec::new();
    let mut round_side_samples = Vec::new();
    let mut economy_raw = Vec::new();

    if !meta_ticks.is_empty() {
        let out = run_parser(
            &mmap,
            &[
                "name",
                "steamid",
                "team_num",
                "cash_spent_this_round",
                "current_equip_value",
                "start_balance",
                "armor",
                "has_helmet",
                "has_defuser",
                "inventory",
            ],
            &["CCSTeam.m_szClanTeamname", "CCSTeam.m_iTeamNum"],
            &[],
            &meta_ticks,
            true,
            false,
        )?;
        let r = ColReader::from_output(&out);
        let name_c = r.get("name");
        let tn_c = r.get("team_num");
        let cs_c = r.get("cash_spent_this_round");
        let eq_c = r.get("current_equip_value");
        let sb_c = r.get("start_balance");
        let ar_c = r.get("armor");
        let hh_c = r.get("has_helmet");
        let hd_c = r.get("has_defuser");
        let inv_c = r.get("inventory");
        let team_num_c = r.get("CCSTeam.m_iTeamNum");
        let clan_c = r.get("CCSTeam.m_szClanTeamname");

        let mut seen_team_at_start = false;
        for i in 0..r.len {
            let tick = col_i64(r.tick_col, i).unwrap_or(0);
            let sid = col_u64(r.steamid_col, i);
            let team_num = col_i64(tn_c, i);

            if tick == match_start_tick {
                player_info.push(PlayerInfoRow {
                    steamid: sid,
                    name: col_str(name_c, i),
                    team_num,
                });
                if !seen_team_at_start {
                    if let (Some(tn), Some(cname)) = (col_i64(team_num_c, i), col_str(clan_c, i)) {
                        let trimmed = cname.trim();
                        if !trimmed.is_empty()
                            && !matches!(
                                trimmed.to_ascii_lowercase().as_str(),
                                "ct" | "terrorist" | "t" | "team a" | "team b"
                            )
                        {
                            if tn == 2 {
                                team_a_name = Some(trimmed.to_string());
                            } else if tn == 3 {
                                team_b_name = Some(trimmed.to_string());
                            }
                            if team_a_name.is_some() && team_b_name.is_some() {
                                seen_team_at_start = true;
                            }
                        }
                    }
                }
            }
            if round_side_tick_set.contains(&tick) {
                round_side_samples.push(SideSampleRow {
                    tick,
                    total_rounds_played: 0,
                    steamid: sid,
                    team_num,
                });
            }
            if freeze_tick_set.contains(&tick) {
                economy_raw.push(EconomyRawRow {
                    tick,
                    steamid: sid,
                    cash_spent_this_round: col_i64(cs_c, i).unwrap_or(0),
                    current_equip_value: col_i64(eq_c, i).unwrap_or(0),
                    start_balance: col_i64(sb_c, i).unwrap_or(0),
                    armor: col_i64(ar_c, i).unwrap_or(0),
                    has_helmet: col_bool(hh_c, i),
                    has_defuser: col_bool(hd_c, i),
                    inventory: col_str_vec_ref(inv_c, i).to_vec(),
                });
            }
        }
    }
    record(&mut timings, "parse.metaAndEconomy", st);

    // Pass 5: replay grid (8 Hz) directly into columnar ReplayColumns
    report("replay grid (slowest stage)", 0.55);
    let st = Instant::now();
    anyhow::ensure!(opts.sample_rate >= 1, "--sample-rate must be >= 1");
    let step = (tickrate / opts.sample_rate).max(1);
    let replay_ticks = build_sample_ticks(&round_ends, &round_freeze_ends, &round_starts, step);
    let mut replay_cols = ReplayColumns::default();
    if !replay_ticks.is_empty() {
        let rt_i32: Vec<i32> = replay_ticks.iter().map(|&t| t as i32).collect();
        let out = run_parser(
            &mmap,
            &[
                "steamid",
                "team_num",
                "X",
                "Y",
                "Z",
                "yaw",
                "pitch",
                "health",
                "armor",
                "active_weapon_name",
                "flash_duration",
                "balance",
                "current_equip_value",
                "has_defuser",
                "last_place_name",
                "inventory",
            ],
            &[],
            &[],
            &rt_i32,
            true,
            false,
        )?;
        let r = ColReader::from_output(&out);
        let x_c = r.get("X");
        let y_c = r.get("Y");
        let z_c = r.get("Z");
        let yaw_c = r.get("yaw");
        let pitch_c = r.get("pitch");
        let hp_c = r.get("health");
        let arm_c = r.get("armor");
        let aw_c = r.get("active_weapon_name");
        let fd_c = r.get("flash_duration");
        let bal_c = r.get("balance");
        let eq_c = r.get("current_equip_value");
        let def_c = r.get("has_defuser");
        let pl_c = r.get("last_place_name");
        let inv_c = r.get("inventory");

        replay_cols.ticks.reserve(r.len);
        replay_cols.steamids.reserve(r.len);
        replay_cols.x.reserve(r.len);
        replay_cols.y.reserve(r.len);
        replay_cols.z.reserve(r.len);
        replay_cols.yaw.reserve(r.len);
        replay_cols.pitch.reserve(r.len);
        replay_cols.health.reserve(r.len);
        replay_cols.armor.reserve(r.len);
        replay_cols.flash_duration.reserve(r.len);
        replay_cols.balance.reserve(r.len);
        replay_cols.current_equip_value.reserve(r.len);
        replay_cols.has_defuser.reserve(r.len);
        replay_cols.weapon_codes.reserve(r.len);
        replay_cols.place_codes.reserve(r.len);
        replay_cols.grenades.reserve(r.len);
        replay_cols.has_bomb.reserve(r.len);

        let mut weapon_cache: HashMap<String, i32> = HashMap::new();
        for i in 0..r.len {
            replay_cols.ticks.push(col_i64(r.tick_col, i).unwrap_or(0));
            replay_cols.steamids.push(col_u64(r.steamid_col, i));
            replay_cols.x.push(col_f32(x_c, i));
            replay_cols.y.push(col_f32(y_c, i));
            replay_cols.z.push(col_f32(z_c, i));
            replay_cols.yaw.push(col_f32(yaw_c, i));
            replay_cols.pitch.push(col_f32(pitch_c, i));
            replay_cols.health.push(col_f32(hp_c, i));
            replay_cols.armor.push(col_f32(arm_c, i));
            replay_cols.flash_duration.push(col_f32(fd_c, i));
            replay_cols.balance.push(col_f32(bal_c, i));
            replay_cols.current_equip_value.push(col_f32(eq_c, i));
            replay_cols.has_defuser.push(col_bool(def_c, i));

            let wcode = match col_str_ref(aw_c, i) {
                Some(raw_w) => {
                    if let Some(&c) = weapon_cache.get(raw_w) {
                        c
                    } else {
                        let norm = crate::enums::normalize_weapon_name(Some(raw_w));
                        let c = replay_cols.weapon_dict.index(norm.as_deref());
                        weapon_cache.insert(raw_w.to_string(), c);
                        c
                    }
                }
                None => -1,
            };
            replay_cols.weapon_codes.push(wcode);

            let pcode = col_str_ref(pl_c, i)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| replay_cols.place_dict.index(Some(s)))
                .unwrap_or(-1);
            replay_cols.place_codes.push(pcode);

            replay_cols.push_inventory(col_str_vec_ref(inv_c, i));
        }
    }
    record(&mut timings, "parse.replayGrid", st);

    // Pass 6: grenade trajectories
    report("grenade trajectories", 0.75);
    let st = Instant::now();
    let (grenade_throws, grenade_trajectories) = extract_grenade_paths(&mmap, &replay_ticks)?;
    record(&mut timings, "parse.grenadeTrajectories", st);

    // Pass 7 (--research): duel windows directly into columnar DuelColumns
    let mut duel_cols = DuelColumns::default();
    let mut duel_windows = Vec::new();
    if opts.research {
        report("duel windows (full tick)", 0.85);
        let st = Instant::now();
        let anchor_ticks: Vec<i64> = deaths
            .iter()
            .map(|d| d.tick)
            .chain(hurts.iter().map(|h| h.tick))
            .collect();
        duel_windows = merge_windows(
            &anchor_ticks,
            &round_ends,
            &round_freeze_ends,
            &round_starts,
            tickrate,
            opts.window_before_ms,
            opts.window_after_ms,
        );
        let mut duel_ticks = Vec::new();
        for &(s, e) in &duel_windows {
            for t in s..=e {
                duel_ticks.push(t as i32);
            }
        }
        if !duel_ticks.is_empty() {
            let out = run_parser(
                &mmap,
                &[
                    "steamid",
                    "X",
                    "Y",
                    "Z",
                    "yaw",
                    "pitch",
                    "health",
                    "flash_duration",
                ],
                &[],
                &[],
                &duel_ticks,
                true,
                false,
            )?;
            let r = ColReader::from_output(&out);
            let x_c = r.get("X");
            let y_c = r.get("Y");
            let z_c = r.get("Z");
            let yaw_c = r.get("yaw");
            let pitch_c = r.get("pitch");
            let hp_c = r.get("health");
            let fd_c = r.get("flash_duration");

            duel_cols.ticks.reserve(r.len);
            duel_cols.steamids.reserve(r.len);
            duel_cols.x.reserve(r.len);
            duel_cols.y.reserve(r.len);
            duel_cols.z.reserve(r.len);
            duel_cols.yaw.reserve(r.len);
            duel_cols.pitch.reserve(r.len);
            duel_cols.health.reserve(r.len);
            duel_cols.flash_duration.reserve(r.len);

            for i in 0..r.len {
                duel_cols.ticks.push(col_i64(r.tick_col, i).unwrap_or(0));
                duel_cols.steamids.push(col_u64(r.steamid_col, i));
                duel_cols.x.push(col_f32(x_c, i));
                duel_cols.y.push(col_f32(y_c, i));
                duel_cols.z.push(col_f32(z_c, i));
                duel_cols.yaw.push(col_f32(yaw_c, i));
                duel_cols.pitch.push(col_f32(pitch_c, i));
                duel_cols.health.push(col_f32(hp_c, i));
                duel_cols.flash_duration.push(col_f32(fd_c, i));
            }
        }
        record(&mut timings, "parse.duelWindows", st);
    }

    record(&mut timings, "parse.total", parse_started);

    Ok(RawDemo {
        timings,
        header,
        tickrate,
        sample_rate: (tickrate / step).max(1),
        match_start_tick,
        team_a_name,
        team_b_name,
        player_info,
        round_starts,
        round_freeze_ends,
        round_ends,
        deaths,
        hurts,
        fires,
        blinds,
        bomb_events,
        grenade_detonations,
        inferno_expires,
        smoke_expires,
        grenade_throws,
        grenade_trajectories,
        replay_cols,
        replay_ticks,
        duel_cols,
        duel_windows,
        round_side_samples,
        economy_raw,
        freeze_ticks,
    })
}
