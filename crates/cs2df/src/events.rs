use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::enums::{
    bomb_site_from_place, classify_inventory, normalize_hitgroup, normalize_weapon_name,
    BombEventKind, EconomyType, GrenadeType, TeamKey,
};
use crate::rounds::{valid_sid64, PlayerInfoRow, RoundModel, RoundRow};

#[inline]
pub fn round_ties_even_i64(v: f64) -> i64 {
    if !v.is_finite() {
        0
    } else {
        v.round_ties_even() as i64
    }
}

#[inline]
pub fn round_f64(v: f64, decimals: u32) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    let factor = 10f64.powi(decimals as i32);
    (v * factor).round_ties_even() / factor
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pos3 {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl Pos3 {
    pub fn from_opts(x: Option<f64>, y: Option<f64>, z: Option<f64>) -> Self {
        Self {
            x: round_ties_even_i64(x.filter(|v| v.is_finite()).unwrap_or(0.0)),
            y: round_ties_even_i64(y.filter(|v| v.is_finite()).unwrap_or(0.0)),
            z: round_ties_even_i64(z.filter(|v| v.is_finite()).unwrap_or(0.0)),
        }
    }

    pub fn from_opts_nullable(x: Option<f64>, y: Option<f64>, z: Option<f64>) -> Option<Self> {
        let xf = x.filter(|v| v.is_finite());
        let yf = y.filter(|v| v.is_finite());
        let zf = z.filter(|v| v.is_finite());
        if xf.is_none() && yf.is_none() && zf.is_none() {
            return None;
        }
        Some(Self {
            x: round_ties_even_i64(xf.unwrap_or(0.0)),
            y: round_ties_even_i64(yf.unwrap_or(0.0)),
            z: round_ties_even_i64(zf.unwrap_or(0.0)),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRow {
    #[serde(rename = "steamId64")]
    pub steam_id64: String,
    pub name: String,
    pub team_key: TeamKey,
    pub crosshair_code: Option<String>,
    #[serde(skip)]
    pub steam_id_num: u64,
}

#[derive(Debug, Clone)]
pub struct PlayerDirectory {
    pub rows: Vec<PlayerRow>,
    pub index_by_sid: HashMap<u64, usize>,
    pub team_by_sid: HashMap<u64, TeamKey>,
}

impl PlayerDirectory {
    pub fn new(rows: Vec<PlayerRow>) -> Self {
        let mut index_by_sid = HashMap::with_capacity(rows.len());
        let mut team_by_sid = HashMap::with_capacity(rows.len());
        for (i, r) in rows.iter().enumerate() {
            let sid = r.steam_id_num;
            index_by_sid.insert(sid, i);
            team_by_sid.insert(sid, r.team_key);
        }
        Self {
            rows,
            index_by_sid,
            team_by_sid,
        }
    }

    #[inline]
    pub fn idx(&self, sid: Option<u64>) -> Option<usize> {
        let s = valid_sid64(sid)?;
        self.index_by_sid.get(&s).copied()
    }

    #[inline]
    pub fn team(&self, sid: Option<u64>) -> Option<TeamKey> {
        let s = valid_sid64(sid)?;
        self.team_by_sid.get(&s).copied()
    }

    #[inline]
    pub fn team_of_index(&self, idx: usize) -> TeamKey {
        self.rows[idx].team_key
    }
}

pub fn build_players(player_info: &[PlayerInfoRow]) -> PlayerDirectory {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for r in player_info {
        let Some(sid) = valid_sid64(r.steamid) else {
            continue;
        };
        if !seen.insert(sid) {
            continue;
        }
        let team_key = match r.team_num {
            Some(2) => TeamKey::TeamA,
            Some(3) => TeamKey::TeamB,
            _ => continue,
        };
        let sid_str = sid.to_string();
        let name = r
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or(&sid_str)
            .to_string();
        out.push(PlayerRow {
            steam_id64: sid_str,
            name,
            team_key,
            crosshair_code: r.crosshair_code.clone(),
            steam_id_num: sid,
        });
    }
    out.sort_by(|a, b| {
        a.team_key
            .cmp(&b.team_key)
            .then_with(|| a.steam_id_num.cmp(&b.steam_id_num))
    });
    PlayerDirectory::new(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMatchInfo {
    pub team_key: TeamKey,
    pub name: Option<String>,
    pub score: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchRow {
    pub map_name: String,
    pub tickrate: i64,
    pub duration_seconds: f64,
    pub server_name: Option<String>,
    pub source: &'static str,
    pub team_a: TeamMatchInfo,
    pub team_b: TeamMatchInfo,
}

pub fn build_match(
    header: &HashMap<String, String>,
    tickrate: i64,
    team_a_name_opt: Option<&str>,
    team_b_name_opt: Option<&str>,
    rounds: &[RoundRow],
) -> MatchRow {
    let team_a_score = rounds
        .iter()
        .filter(|r| r.winner_team_key == TeamKey::TeamA)
        .count() as i64;
    let team_b_score = rounds
        .iter()
        .filter(|r| r.winner_team_key == TeamKey::TeamB)
        .count() as i64;

    let team_a_name = team_a_name_opt
        .or_else(|| header.get("team_name_t").map(String::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let team_b_name = team_b_name_opt
        .or_else(|| header.get("team_name_ct").map(String::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut duration = header
        .get("playback_time")
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0);
    if duration == 0.0 {
        let last_tick = rounds.iter().map(|r| r.end_tick).max().unwrap_or(0);
        let tr = tickrate.max(1) as f64;
        duration = round_f64(last_tick as f64 / tr, 1);
    }
    if duration <= 0.0 {
        duration = 1.0;
    }

    let map_name = header
        .get("map_name")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string();
    let server_name = header
        .get("server_name")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    MatchRow {
        map_name,
        tickrate,
        duration_seconds: duration,
        server_name,
        source: "demo",
        team_a: TeamMatchInfo {
            team_key: TeamKey::TeamA,
            name: team_a_name,
            score: team_a_score,
        },
        team_b: TeamMatchInfo {
            team_key: TeamKey::TeamB,
            name: team_b_name,
            score: team_b_score,
        },
    }
}

fn active_event_round_number(
    round_model: &RoundModel,
    tick: i64,
    total_rounds_played: i64,
) -> Option<i64> {
    let n = round_model.round_for_event(tick, total_rounds_played)?;
    if !round_model.has_round(n) {
        return None;
    }
    let window = round_model.window_for_round(n)?;
    let event_end = round_model.event_end_tick(n)?;
    if tick < window.freeze_end_tick || tick > event_end {
        return None;
    }
    Some(n)
}

#[derive(Debug, Clone)]
pub struct DeathRawRow {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub user_steamid: Option<u64>,
    pub attacker_steamid: Option<u64>,
    pub assister_steamid: Option<u64>,
    pub weapon: Option<String>,
    pub attacker_active_weapon: Option<String>,
    pub user_active_weapon: Option<String>,
    pub headshot: bool,
    pub assistedflash: bool,
    pub thrusmoke: bool,
    pub noscope: bool,
    pub penetrated: i64,
    pub attackerblind: Option<bool>,
    pub attacker_x: Option<f64>,
    pub attacker_y: Option<f64>,
    pub attacker_z: Option<f64>,
    pub user_x: Option<f64>,
    pub user_y: Option<f64>,
    pub user_z: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KillRow {
    pub round_number: i64,
    pub tick: i64,
    pub killer_index: Option<usize>,
    pub victim_index: usize,
    pub assister_index: Option<usize>,
    pub flash_assister_index: Option<usize>,
    pub weapon: String,
    pub killer_active_weapon: Option<String>,
    pub victim_active_weapon: Option<String>,
    pub headshot: bool,
    pub flash_assist: bool,
    pub trade_kill: bool,
    pub trade_death: bool,
    pub through_smoke: bool,
    pub no_scope: bool,
    pub penetrated_objects: i64,
    pub killer_position: Option<Pos3>,
    pub victim_position: Pos3,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub killer_blinded: Option<bool>,
}

pub fn build_kills(
    deaths: &[DeathRawRow],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    tickrate: i64,
) -> Vec<KillRow> {
    let mut out = Vec::new();
    for r in deaths {
        let Some(n) = active_event_round_number(round_model, r.tick, r.total_rounds_played) else {
            continue;
        };
        let Some(victim_idx) = players.idx(r.user_steamid) else {
            continue;
        };
        let victim_team = players.team_of_index(victim_idx);
        if !round_model.side_map.contains_key(&(n, victim_team)) {
            continue;
        }
        let weapon = r.weapon.as_deref().unwrap_or("").to_string();
        if weapon.is_empty() {
            continue;
        }

        let killer_idx = players.idx(r.attacker_steamid);
        let assist_idx = players.idx(r.assister_steamid);
        let flash_assister_idx = if r.assistedflash { assist_idx } else { None };

        out.push(KillRow {
            round_number: n,
            tick: r.tick,
            killer_index: killer_idx,
            victim_index: victim_idx,
            assister_index: assist_idx,
            flash_assister_index: flash_assister_idx,
            weapon,
            killer_active_weapon: normalize_weapon_name(r.attacker_active_weapon.as_deref()),
            victim_active_weapon: normalize_weapon_name(r.user_active_weapon.as_deref()),
            headshot: r.headshot,
            flash_assist: r.assistedflash && flash_assister_idx.is_some(),
            trade_kill: false,
            trade_death: false,
            through_smoke: r.thrusmoke,
            no_scope: r.noscope,
            penetrated_objects: r.penetrated,
            killer_position: Pos3::from_opts_nullable(r.attacker_x, r.attacker_y, r.attacker_z),
            victim_position: Pos3::from_opts(r.user_x, r.user_y, r.user_z),
            killer_blinded: r.attackerblind,
        });
    }
    let trade_window = 6 * tickrate.max(1);
    annotate_trades(&mut out, players, trade_window);
    out
}

pub fn annotate_trades(kills: &mut [KillRow], players: &PlayerDirectory, trade_window_ticks: i64) {
    kills.sort_by_key(|k| (k.round_number, k.tick));
    for k in kills.iter_mut() {
        k.trade_kill = false;
        k.trade_death = false;
    }

    let enemy_kill = |k: &KillRow| -> bool {
        k.killer_index
            .is_some_and(|ki| players.team_of_index(ki) != players.team_of_index(k.victim_index))
    };

    for i in 0..kills.len() {
        if !enemy_kill(&kills[i]) {
            continue;
        }
        let kill_round = kills[i].round_number;
        let kill_tick = kills[i].tick;
        let kill_killer = kills[i].killer_index.unwrap();
        let kill_victim = kills[i].victim_index;

        let mut j = i;
        while j > 0 {
            j -= 1;
            let delta = kill_tick - kills[j].tick;
            if kills[j].round_number != kill_round || delta > trade_window_ticks {
                break;
            }
            if delta >= 0
                && enemy_kill(&kills[j])
                && kills[j].killer_index == Some(kill_victim)
                && players.team_of_index(kill_killer)
                    == players.team_of_index(kills[j].victim_index)
            {
                kills[i].trade_kill = true;
                kills[j].trade_death = true;
                break;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct HurtRawRow {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub user_steamid: Option<u64>,
    pub attacker_steamid: Option<u64>,
    pub weapon: Option<String>,
    pub hitgroup: Option<String>,
    pub dmg_health: i64,
    pub dmg_armor: i64,
    pub armor: i64,
    pub attacker_x: Option<f64>,
    pub attacker_y: Option<f64>,
    pub attacker_z: Option<f64>,
    pub user_x: Option<f64>,
    pub user_y: Option<f64>,
    pub user_z: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DamageRow {
    pub round_number: i64,
    pub tick: i64,
    pub attacker_index: Option<usize>,
    pub victim_index: usize,
    pub weapon: String,
    pub hitgroup: &'static str,
    pub health_damage: i64,
    pub health_damage_raw: i64,
    pub armor_damage: i64,
    pub victim_health_before: i64,
    pub victim_armor_after: i64,
    pub attacker_position: Option<Pos3>,
    pub victim_position: Pos3,
}

pub fn build_damages(
    hurts: &[HurtRawRow],
    players: &PlayerDirectory,
    round_model: &RoundModel,
) -> Vec<DamageRow> {
    let mut sorted: Vec<&HurtRawRow> = hurts.iter().collect();
    sorted.sort_by_key(|r| r.tick);

    let mut out = Vec::new();
    let mut remaining_health: HashMap<(i64, usize), i64> = HashMap::new();

    for r in sorted {
        let Some(n) = active_event_round_number(round_model, r.tick, r.total_rounds_played) else {
            continue;
        };
        let Some(vic_idx) = players.idx(r.user_steamid) else {
            continue;
        };
        let vic_team = players.team_of_index(vic_idx);
        if !round_model.side_map.contains_key(&(n, vic_team)) {
            continue;
        }
        let weapon = r.weapon.as_deref().unwrap_or("").to_string();
        if weapon.is_empty() {
            continue;
        }

        let atk_idx = players.idx(r.attacker_steamid);

        let raw_dmg = r.dmg_health;
        let health_before = *remaining_health.entry((n, vic_idx)).or_insert(100);
        let health_dmg = raw_dmg.max(0).min(health_before);
        remaining_health.insert((n, vic_idx), health_before - health_dmg);
        let armor_after = r.armor.min(100);

        out.push(DamageRow {
            round_number: n,
            tick: r.tick,
            attacker_index: atk_idx,
            victim_index: vic_idx,
            weapon,
            hitgroup: normalize_hitgroup(r.hitgroup.as_deref()),
            health_damage: health_dmg,
            health_damage_raw: raw_dmg,
            armor_damage: r.dmg_armor,
            victim_health_before: health_before,
            victim_armor_after: armor_after,
            attacker_position: Pos3::from_opts_nullable(r.attacker_x, r.attacker_y, r.attacker_z),
            victim_position: Pos3::from_opts(r.user_x, r.user_y, r.user_z),
        });
    }
    out
}

#[derive(Debug, Clone)]
pub struct BlindRawRow {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub attacker_steamid: Option<u64>,
    pub user_steamid: Option<u64>,
    pub blind_duration: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlindRow {
    pub round_number: i64,
    pub tick: i64,
    pub flash_id: Option<String>,
    pub flasher_index: usize,
    pub flashed_index: usize,
    pub duration_seconds: f64,
}

pub fn build_blinds(
    blinds: &[BlindRawRow],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    flash_lookup: &HashMap<(i64, i64), String>,
) -> Vec<BlindRow> {
    let mut out = Vec::new();
    for r in blinds {
        let Some(n) = active_event_round_number(round_model, r.tick, r.total_rounds_played) else {
            continue;
        };
        let Some(flasher_idx) = players.idx(r.attacker_steamid) else {
            continue;
        };
        let Some(flashed_idx) = players.idx(r.user_steamid) else {
            continue;
        };
        let flasher_team = players.team_of_index(flasher_idx);
        let flashed_team = players.team_of_index(flashed_idx);
        if !round_model.side_map.contains_key(&(n, flasher_team))
            || !round_model.side_map.contains_key(&(n, flashed_team))
        {
            continue;
        }
        let dur = if r.blind_duration.is_finite() {
            r.blind_duration.min(6.0)
        } else {
            0.0
        };
        out.push(BlindRow {
            round_number: n,
            tick: r.tick,
            flash_id: flash_lookup.get(&(n, r.tick)).cloned(),
            flasher_index: flasher_idx,
            flashed_index: flashed_idx,
            duration_seconds: round_f64(dur, 3),
        });
    }
    out
}

#[derive(Debug, Clone)]
pub struct BombRawEvent {
    pub kind: BombEventKind,
    pub tick: i64,
    pub total_rounds_played: i64,
    pub user_steamid: Option<u64>,
    pub user_x: Option<f64>,
    pub user_y: Option<f64>,
    pub user_z: Option<f64>,
    pub user_last_place_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BombRow {
    pub round_number: i64,
    pub tick: i64,
    #[serde(rename = "type")]
    pub event_type: BombEventKind,
    pub site: Option<&'static str>,
    pub actor_index: Option<usize>,
    pub position: Pos3,
}

pub fn build_bombs(
    bomb_events: &[BombRawEvent],
    players: &PlayerDirectory,
    round_model: &RoundModel,
) -> Vec<BombRow> {
    let mut round_site: HashMap<i64, &'static str> = HashMap::new();
    for r in bomb_events {
        if r.kind == BombEventKind::Planted {
            let n = round_model.round_for_event(r.tick, r.total_rounds_played);
            let site = bomb_site_from_place(r.user_last_place_name.as_deref());
            if let (Some(rn), Some(st)) = (n, site) {
                round_site.insert(rn, st);
            }
        }
    }

    let mut out = Vec::with_capacity(bomb_events.len());
    for r in bomb_events {
        let Some(n) = round_model.round_for_event(r.tick, r.total_rounds_played) else {
            continue;
        };
        let Some(window) = round_model.window_for_round(n) else {
            continue;
        };
        let Some(event_end) = round_model.event_end_tick(n) else {
            continue;
        };
        if r.tick < window.freeze_end_tick || r.tick > event_end {
            continue;
        }
        let is_round_site_type = matches!(
            r.kind,
            BombEventKind::PlantAbort
                | BombEventKind::Planted
                | BombEventKind::DefuseBegin
                | BombEventKind::DefuseAbort
                | BombEventKind::Defused
                | BombEventKind::Exploded
        );
        let mut site = bomb_site_from_place(r.user_last_place_name.as_deref());
        if site.is_none() && is_round_site_type {
            site = round_site.get(&n).copied();
        }
        out.push(BombRow {
            round_number: n,
            tick: r.tick,
            event_type: r.kind,
            site,
            actor_index: players.idx(r.user_steamid),
            position: Pos3::from_opts(r.user_x, r.user_y, r.user_z),
        });
    }
    out.sort_by_key(|b| (b.round_number, b.tick));
    out
}

#[derive(Debug, Clone)]
pub struct GrenadeThrowRaw {
    pub grenade_entity_id: Option<i64>,
    pub grenade: GrenadeType,
    pub tick: i64,
    pub destroy_tick: i64,
    pub steamid: Option<u64>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone)]
pub struct GrenadeDetonationRaw {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub grenade_type: GrenadeType,
    pub user_steamid: Option<u64>,
    pub entity_id: Option<i64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GrenadeExpireRaw {
    pub tick: i64,
    pub total_rounds_played: i64,
    pub entity_id: Option<i64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct InfernoFireRaw {
    pub entity_id: i64,
    pub start_tick: i64,
    pub end_tick: i64,
    pub steamid: Option<u64>,
    pub fire_points: Vec<Pos3>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrenadeRow {
    pub round_number: i64,
    pub grenade_id: Option<String>,
    pub throw_tick: i64,
    pub effect_tick: i64,
    pub destroy_tick: Option<i64>,
    #[serde(skip)]
    pub entity_id: Option<i64>,
    pub grenade: GrenadeType,
    pub thrower_index: usize,
    pub throw_position: Pos3,
    pub effect_position: Pos3,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fire_points: Option<Vec<Pos3>>,
}

pub fn build_grenades(
    grenade_throws: &[GrenadeThrowRaw],
    grenade_detonations: &[GrenadeDetonationRaw],
    inferno_expires: &[GrenadeExpireRaw],
    smoke_expires: &[GrenadeExpireRaw],
    inferno_fires: &[InfernoFireRaw],
    players: &PlayerDirectory,
    round_model: &RoundModel,
) -> Vec<GrenadeRow> {
    struct ThrowItem {
        rn: i64,
        tick: i64,
        destroy_tick: Option<i64>,
        gtype: GrenadeType,
        sid: Option<u64>,
        eid: Option<String>,
        pos: Pos3,
    }

    let mut throws = Vec::new();
    for r in grenade_throws {
        let Some(n) = round_model.round_for_tick(r.tick) else {
            continue;
        };
        let eid = r.grenade_entity_id.map(|id| format!("{id}-{}", r.tick));
        throws.push(ThrowItem {
            rn: n,
            tick: r.tick,
            destroy_tick: if r.destroy_tick > 0 {
                Some(r.destroy_tick)
            } else {
                None
            },
            gtype: r.grenade,
            sid: valid_sid64(r.steamid),
            eid,
            pos: Pos3::from_opts(Some(r.x), Some(r.y), Some(r.z)),
        });
    }
    throws.sort_by_key(|t| t.tick);

    let mut out = Vec::new();
    for r in grenade_detonations {
        let Some(n) = round_model.round_for_event(r.tick, r.total_rounds_played) else {
            continue;
        };
        let Some(window) = round_model.window_for_round(n) else {
            continue;
        };
        let Some(event_end) = round_model.event_end_tick(n) else {
            continue;
        };
        if r.tick < window.freeze_end_tick || r.tick > event_end {
            continue;
        }

        let mut thrower_sid = valid_sid64(r.user_steamid);
        let matched = throws
            .iter()
            .filter(|t| {
                t.rn == n
                    && t.gtype == r.grenade_type
                    && t.tick <= r.tick
                    && (thrower_sid.is_none() || t.sid == thrower_sid)
            })
            .max_by_key(|t| t.tick);

        let detonate_entity_id = r.entity_id;
        let (throw_pos, throw_tick, mut destroy_tick, grenade_id) = if let Some(m) = matched {
            if thrower_sid.is_none() {
                thrower_sid = m.sid;
            }
            (
                m.pos.clone(),
                m.tick,
                if r.grenade_type == GrenadeType::Smoke {
                    None
                } else {
                    m.destroy_tick
                },
                m.eid.clone(),
            )
        } else {
            (
                Pos3::from_opts(r.x, r.y, r.z),
                r.tick,
                None,
                detonate_entity_id.map(|id| format!("{id}-{}", r.tick)),
            )
        };

        if throw_tick <= 0 || r.tick <= 0 {
            continue;
        }
        let Some(thrower_idx) = players.idx(thrower_sid) else {
            continue;
        };
        let thrower_team = players.team_of_index(thrower_idx);
        if !round_model.side_map.contains_key(&(n, thrower_team)) {
            continue;
        }
        if let Some(dt) = destroy_tick {
            if dt < r.tick || dt > event_end {
                destroy_tick = None;
            }
        }

        out.push(GrenadeRow {
            round_number: n,
            grenade_id,
            throw_tick,
            effect_tick: r.tick,
            destroy_tick,
            entity_id: detonate_entity_id,
            grenade: r.grenade_type,
            thrower_index: thrower_idx,
            throw_position: throw_pos,
            effect_position: Pos3::from_opts(r.x, r.y, r.z),
            fire_points: None,
        });
    }

    // molotov burn end: pair with nearest later inferno_expire in-round
    struct ExpireItem {
        rn: i64,
        tick: i64,
        pos: Pos3,
        used: bool,
    }
    let mut expires: Vec<ExpireItem> = inferno_expires
        .iter()
        .filter_map(|r| {
            let n = round_model.round_for_event(r.tick, r.total_rounds_played)?;
            if r.tick <= 0 {
                return None;
            }
            Some(ExpireItem {
                rn: n,
                tick: r.tick,
                pos: Pos3::from_opts(r.x, r.y, r.z),
                used: false,
            })
        })
        .collect();
    expires.sort_by_key(|e| e.tick);

    for g in out.iter_mut() {
        if g.grenade != GrenadeType::Molotov || g.destroy_tick.is_some() {
            continue;
        }
        let event_end = round_model.event_end_tick(g.round_number);
        let mut best_idx: Option<usize> = None;
        let mut best_d2: Option<i128> = None;
        for (idx, e) in expires.iter().enumerate() {
            if e.used || e.rn != g.round_number || e.tick < g.effect_tick {
                continue;
            }
            if event_end.is_some_and(|ee| e.tick > ee) {
                continue;
            }
            let dx = (e.pos.x - g.effect_position.x) as i128;
            let dy = (e.pos.y - g.effect_position.y) as i128;
            let d2 = dx * dx + dy * dy;
            if best_d2.is_none_or(|b| d2 < b) {
                best_idx = Some(idx);
                best_d2 = Some(d2);
            }
        }
        if let Some(idx) = best_idx {
            expires[idx].used = true;
            g.destroy_tick = Some(expires[idx].tick);
        }
    }

    // smoke lifetime from smokegrenade_expired (same entity id)
    let mut smoke_index: HashMap<(i64, i64), Vec<i64>> = HashMap::new();
    for r in smoke_expires {
        let Some(n) = round_model.round_for_event(r.tick, r.total_rounds_played) else {
            continue;
        };
        let Some(eid) = r.entity_id else {
            continue;
        };
        if r.tick <= 0 {
            continue;
        }
        smoke_index.entry((n, eid)).or_default().push(r.tick);
    }
    for lst in smoke_index.values_mut() {
        lst.sort_unstable();
    }

    for g in out.iter_mut() {
        let Some(eid) = g.entity_id.take() else {
            continue;
        };
        if g.grenade != GrenadeType::Smoke {
            continue;
        }
        let event_end = round_model.event_end_tick(g.round_number);
        if let Some(ticks) = smoke_index.get(&(g.round_number, eid)) {
            for &t in ticks {
                if t < g.effect_tick {
                    continue;
                }
                if event_end.is_some_and(|ee| t > ee) {
                    continue;
                }
                g.destroy_tick = Some(t);
                break;
            }
        }
    }

    // attach CInferno fire points to molotov / incendiary grenades:
    // 1) prefer direct entity_id match;
    // 2) fallback to spatiotemporal match using fire centroid with explicit distance cap.
    struct FireItem<'a> {
        rn: i64,
        thrower_idx: Option<usize>,
        centroid_x: i64,
        centroid_y: i64,
        raw: &'a InfernoFireRaw,
        used: bool,
    }
    let mut fire_items: Vec<FireItem<'_>> = inferno_fires
        .iter()
        .filter_map(|f| {
            if f.fire_points.is_empty() {
                return None;
            }
            let rn = round_model.round_for_tick(f.start_tick)?;
            let n_pts = f.fire_points.len() as i128;
            let sum_x: i128 = f.fire_points.iter().map(|p| p.x as i128).sum();
            let sum_y: i128 = f.fire_points.iter().map(|p| p.y as i128).sum();
            Some(FireItem {
                rn,
                thrower_idx: players.idx(f.steamid),
                centroid_x: (sum_x / n_pts) as i64,
                centroid_y: (sum_y / n_pts) as i64,
                raw: f,
                used: false,
            })
        })
        .collect();

    // Pass A: direct entity_id match
    for g in out.iter_mut() {
        if !matches!(g.grenade, GrenadeType::Molotov | GrenadeType::Incendiary) {
            continue;
        }
        let Some(geid) = g.entity_id else {
            continue;
        };
        let mut best_idx: Option<usize> = None;
        let mut best_dt = i64::MAX;
        for (idx, f) in fire_items.iter().enumerate() {
            if f.used || f.rn != g.round_number || f.raw.entity_id != geid {
                continue;
            }
            let dt = (f.raw.start_tick - g.effect_tick).abs();
            if dt <= 96 && dt < best_dt {
                best_dt = dt;
                best_idx = Some(idx);
            }
        }
        if let Some(idx) = best_idx {
            fire_items[idx].used = true;
            g.fire_points = Some(fire_items[idx].raw.fire_points.clone());
        }
    }

    // Pass B: spatiotemporal fallback with centroid distance cap (512 game units)
    const MAX_FIRE_DIST_SQ: i128 = 512 * 512;
    for g in out.iter_mut() {
        if !matches!(g.grenade, GrenadeType::Molotov | GrenadeType::Incendiary)
            || g.fire_points.is_some()
        {
            continue;
        }
        let mut best_idx: Option<usize> = None;
        let mut best_score: Option<(i64, i128)> = None;
        for (idx, f) in fire_items.iter().enumerate() {
            if f.used || f.rn != g.round_number {
                continue;
            }
            if let Some(ti) = f.thrower_idx {
                if ti != g.thrower_index {
                    continue;
                }
            }
            let dt = (f.raw.start_tick - g.effect_tick).abs();
            if dt > 96 {
                continue;
            }
            let dx = (f.centroid_x - g.effect_position.x) as i128;
            let dy = (f.centroid_y - g.effect_position.y) as i128;
            let d2 = dx * dx + dy * dy;
            if d2 > MAX_FIRE_DIST_SQ {
                continue;
            }
            let score = (dt, d2);
            if best_score.is_none_or(|b| score < b) {
                best_score = Some(score);
                best_idx = Some(idx);
            }
        }
        if let Some(idx) = best_idx {
            fire_items[idx].used = true;
            g.fire_points = Some(fire_items[idx].raw.fire_points.clone());
        }
    }

    // Clear temporary entity_id for molotovs too
    for g in out.iter_mut() {
        g.entity_id = None;
    }

    out
}

#[derive(Debug, Clone)]
pub struct WeaponSpanRaw {
    pub entity_id: i64,
    pub weapon: String,
    pub prev_owner_steamid: Option<u64>,
    pub start_tick: i64,
    pub end_tick: i64,
    pub start_x: i64,
    pub start_y: i64,
    pub start_z: i64,
    pub end_x: i64,
    pub end_y: i64,
    pub end_z: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaponSpanRow {
    pub round_number: i64,
    pub entity_id: i64,
    pub weapon: String,
    pub start_tick: i64,
    pub end_tick: i64,
    pub previous_owner_index: Option<usize>,
    pub start_position: Pos3,
    pub end_position: Pos3,
}

pub fn build_weapons(
    spans: &[WeaponSpanRaw],
    players: &PlayerDirectory,
    round_model: &RoundModel,
) -> Vec<WeaponSpanRow> {
    let mut out = Vec::new();
    for s in spans {
        let Some(rn) = round_model.round_for_tick(s.start_tick) else {
            continue;
        };
        let Some(window) = round_model.window_for_round(rn) else {
            continue;
        };
        let Some(event_end) = round_model.event_end_tick(rn) else {
            continue;
        };
        let start_tick = s.start_tick.max(window.freeze_end_tick);
        let end_tick = s.end_tick.min(event_end);
        if start_tick > end_tick {
            continue;
        }
        let Some(weapon) = normalize_weapon_name(Some(&s.weapon)) else {
            continue;
        };
        if weapon == "c4" {
            continue;
        }
        out.push(WeaponSpanRow {
            round_number: rn,
            entity_id: s.entity_id,
            weapon,
            start_tick,
            end_tick,
            previous_owner_index: players.idx(s.prev_owner_steamid),
            start_position: Pos3 {
                x: s.start_x,
                y: s.start_y,
                z: s.start_z,
            },
            end_position: Pos3 {
                x: s.end_x,
                y: s.end_y,
                z: s.end_z,
            },
        });
    }
    out.sort_by_key(|w| (w.round_number, w.start_tick, w.entity_id));
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClutchRow {
    pub round_number: i64,
    pub tick: i64,
    pub clutcher_index: usize,
    pub opponent_count: usize,
    pub won: bool,
    pub survived: bool,
    pub kill_count: usize,
}

pub fn build_clutches(
    kills: &[KillRow],
    rounds: &[RoundRow],
    players: &PlayerDirectory,
) -> Vec<ClutchRow> {
    let mut out = Vec::new();
    let mut kills_by_round: HashMap<i64, Vec<&KillRow>> = HashMap::new();
    for k in kills {
        kills_by_round.entry(k.round_number).or_default().push(k);
    }

    let mut team_a_players = HashSet::new();
    let mut team_b_players = HashSet::new();
    for (i, p) in players.rows.iter().enumerate() {
        match p.team_key {
            TeamKey::TeamA => {
                team_a_players.insert(i);
            }
            TeamKey::TeamB => {
                team_b_players.insert(i);
            }
        }
    }

    for rnd in rounds {
        let Some(mut rnd_kills) = kills_by_round.remove(&rnd.round_number) else {
            continue;
        };
        rnd_kills.sort_by_key(|k| k.tick);

        let mut clutch_detected = HashSet::new();
        let mut dead = HashSet::new();
        for k in &rnd_kills {
            dead.insert(k.victim_index);
            let a_alive: Vec<usize> = team_a_players.difference(&dead).copied().collect();
            let b_alive: Vec<usize> = team_b_players.difference(&dead).copied().collect();

            for (team_key, own_alive, opp_alive) in [
                (TeamKey::TeamA, &a_alive, &b_alive),
                (TeamKey::TeamB, &b_alive, &a_alive),
            ] {
                if own_alive.len() != 1 || opp_alive.is_empty() {
                    continue;
                }
                let idx = own_alive[0];
                if !clutch_detected.insert(idx) {
                    continue;
                }
                let remaining_kills = rnd_kills
                    .iter()
                    .filter(|kk| kk.tick >= k.tick && kk.killer_index == Some(idx))
                    .count();
                out.push(ClutchRow {
                    round_number: rnd.round_number,
                    tick: k.tick,
                    clutcher_index: idx,
                    opponent_count: opp_alive.len(),
                    won: rnd.winner_team_key == team_key,
                    survived: !dead.contains(&idx),
                    kill_count: remaining_kills.min(5),
                });
            }
        }
    }
    out
}

#[derive(Debug, Clone)]
pub struct EconomyRawRow {
    pub tick: i64,
    pub steamid: Option<u64>,
    pub cash_spent_this_round: i64,
    pub current_equip_value: i64,
    pub start_balance: i64,
    pub armor: i64,
    pub has_helmet: bool,
    pub has_defuser: bool,
    pub inventory: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EconomyRow {
    pub round_number: i64,
    pub player_index: usize,
    pub start_money: i64,
    pub money_spent: i64,
    pub equipment_value: i64,
    #[serde(rename = "type")]
    pub eco_type: EconomyType,
    pub has_armor: bool,
    pub has_helmet: bool,
    pub has_defuse_kit: bool,
    pub primary_weapon: Option<String>,
    pub secondary_weapon: Option<String>,
    pub grenade_count: usize,
    pub grenades: Vec<GrenadeType>,
}

const ECO_ORDER: [EconomyType; 5] = [
    EconomyType::Pistol,
    EconomyType::Eco,
    EconomyType::Semi,
    EconomyType::Force,
    EconomyType::Full,
];

fn is_pistol_round(round_number: i64) -> bool {
    matches!(round_number, 1 | 13)
}

fn is_pistol_conversion_round(round_number: i64, team_key: TeamKey, rounds: &[RoundRow]) -> bool {
    let prev = round_number - 1;
    if !matches!(prev, 1 | 13) {
        return false;
    }
    rounds
        .iter()
        .find(|r| r.round_number == prev)
        .is_some_and(|r| r.winner_team_key == team_key)
}

fn economy_type(
    money_spent: i64,
    start_money: i64,
    equipment_value: i64,
    round_number: i64,
) -> EconomyType {
    if is_pistol_round(round_number) {
        return EconomyType::Pistol;
    }
    if equipment_value >= 4000 {
        return EconomyType::Full;
    }
    if money_spent < 1000 && equipment_value < 1000 {
        return EconomyType::Eco;
    }
    if start_money > 0 && (money_spent as f64) / (start_money as f64) >= 0.80 {
        return EconomyType::Force;
    }
    EconomyType::Semi
}

fn team_economy_vote(types: &[EconomyType]) -> EconomyType {
    if types.is_empty() {
        return EconomyType::Semi;
    }
    let mut counts = [0usize; 5];
    for t in types {
        if let Some(pos) = ECO_ORDER.iter().position(|e| e == t) {
            counts[pos] += 1;
        }
    }
    let max_count = counts.iter().copied().max().unwrap_or(0);
    for (i, &c) in counts.iter().enumerate() {
        if c == max_count {
            return ECO_ORDER[i];
        }
    }
    EconomyType::Semi
}

pub fn build_economies(
    economy_raw: &[EconomyRawRow],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    rounds: &mut [RoundRow],
) -> Vec<EconomyRow> {
    let mut freeze_tick_to_round: HashMap<i64, i64> = HashMap::new();
    for w in &round_model.windows {
        freeze_tick_to_round.insert(w.freeze_end_tick, w.round_number);
    }

    let mut out = Vec::new();
    let mut team_round_types: HashMap<(i64, TeamKey), Vec<EconomyType>> = HashMap::new();

    for r in economy_raw {
        let n = freeze_tick_to_round.get(&r.tick).copied().unwrap_or(0);
        if n <= 0 {
            continue;
        }
        let Some(idx) = players.idx(r.steamid) else {
            continue;
        };
        let key = players.team_of_index(idx);
        if !round_model.side_map.contains_key(&(n, key)) {
            continue;
        }

        let eco_type = economy_type(
            r.cash_spent_this_round,
            r.start_balance,
            r.current_equip_value,
            n,
        );
        let (primary, secondary, grenade_count, grenades) = classify_inventory(&r.inventory);

        out.push(EconomyRow {
            round_number: n,
            player_index: idx,
            start_money: r.start_balance,
            money_spent: r.cash_spent_this_round,
            equipment_value: r.current_equip_value,
            eco_type,
            has_armor: r.armor > 0,
            has_helmet: r.has_helmet,
            has_defuse_kit: r.has_defuser,
            primary_weapon: primary,
            secondary_weapon: secondary,
            grenade_count,
            grenades,
        });
        team_round_types.entry((n, key)).or_default().push(eco_type);
    }

    let mut votes: HashMap<(i64, TeamKey), EconomyType> = HashMap::new();
    for ((rn, key), types) in &team_round_types {
        let mut vote = team_economy_vote(types);
        if is_pistol_conversion_round(*rn, *key, rounds) {
            vote = EconomyType::Full;
        }
        votes.insert((*rn, *key), vote);
    }

    for rd in rounds.iter_mut() {
        if let Some(&v) = votes.get(&(rd.round_number, TeamKey::TeamA)) {
            rd.team_a_economy = v;
        }
        if let Some(&v) = votes.get(&(rd.round_number, TeamKey::TeamB)) {
            rd.team_b_economy = v;
        }
    }

    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatsRow {
    pub player_index: usize,
    pub rounds: usize,
    pub kills: usize,
    pub deaths: usize,
    pub assists: usize,
    pub damage_health: i64,
    pub damage_armor: i64,
    pub utility_damage: i64,
    pub headshot_count: usize,
    pub first_kill_count: usize,
    pub first_death_count: usize,
    pub trade_kill_count: usize,
    pub trade_death_count: usize,
    pub no_scope_kill_count: usize,
    pub wallbang_kill_count: usize,
    pub collateral_kill_count: usize,
    pub bomb_plant_count: usize,
    pub bomb_defuse_count: usize,
    pub one_kill_count: usize,
    pub two_kill_count: usize,
    pub three_kill_count: usize,
    pub four_kill_count: usize,
    pub five_kill_count: usize,
    pub vs_one_count: usize,
    pub vs_one_won_count: usize,
    pub vs_one_lost_count: usize,
    pub vs_two_count: usize,
    pub vs_two_won_count: usize,
    pub vs_two_lost_count: usize,
    pub vs_three_count: usize,
    pub vs_three_won_count: usize,
    pub vs_three_lost_count: usize,
    pub vs_four_count: usize,
    pub vs_four_won_count: usize,
    pub vs_four_lost_count: usize,
    pub vs_five_count: usize,
    pub vs_five_won_count: usize,
    pub vs_five_lost_count: usize,
    pub kast_rounds: usize,
    pub flash_assist_count: usize,
    pub enemy_flash_duration_seconds: f64,
    pub team_flash_duration_seconds: f64,
    pub combat_death_count: usize,
    pub bomb_death_count: usize,
    pub adr: f64,
    pub kast: f64,
    pub average_utility_damage_per_round: f64,
}

struct StatsAccum {
    row: PlayerStatsRow,
    rounds_with_kill: HashSet<i64>,
    rounds_with_death: HashSet<i64>,
    rounds_with_assist: HashSet<i64>,
    rounds_traded: HashSet<i64>,
}

impl StatsAccum {
    fn new(idx: usize, total_rounds: usize) -> Self {
        Self {
            row: PlayerStatsRow {
                player_index: idx,
                rounds: total_rounds,
                kills: 0,
                deaths: 0,
                assists: 0,
                damage_health: 0,
                damage_armor: 0,
                utility_damage: 0,
                headshot_count: 0,
                first_kill_count: 0,
                first_death_count: 0,
                trade_kill_count: 0,
                trade_death_count: 0,
                no_scope_kill_count: 0,
                wallbang_kill_count: 0,
                collateral_kill_count: 0,
                bomb_plant_count: 0,
                bomb_defuse_count: 0,
                one_kill_count: 0,
                two_kill_count: 0,
                three_kill_count: 0,
                four_kill_count: 0,
                five_kill_count: 0,
                vs_one_count: 0,
                vs_one_won_count: 0,
                vs_one_lost_count: 0,
                vs_two_count: 0,
                vs_two_won_count: 0,
                vs_two_lost_count: 0,
                vs_three_count: 0,
                vs_three_won_count: 0,
                vs_three_lost_count: 0,
                vs_four_count: 0,
                vs_four_won_count: 0,
                vs_four_lost_count: 0,
                vs_five_count: 0,
                vs_five_won_count: 0,
                vs_five_lost_count: 0,
                kast_rounds: 0,
                flash_assist_count: 0,
                enemy_flash_duration_seconds: 0.0,
                team_flash_duration_seconds: 0.0,
                combat_death_count: 0,
                bomb_death_count: 0,
                adr: 0.0,
                kast: 0.0,
                average_utility_damage_per_round: 0.0,
            },
            rounds_with_kill: HashSet::new(),
            rounds_with_death: HashSet::new(),
            rounds_with_assist: HashSet::new(),
            rounds_traded: HashSet::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_player_stats(
    bomb_events: &[BombRawEvent],
    players: &PlayerDirectory,
    round_model: &RoundModel,
    rounds: &[RoundRow],
    kills: &[KillRow],
    blinds: &[BlindRow],
    damages: &[DamageRow],
    clutches: &[ClutchRow],
) -> Vec<PlayerStatsRow> {
    let total_rounds = rounds.len();
    let mut stats: Vec<StatsAccum> = (0..players.rows.len())
        .map(|i| StatsAccum::new(i, total_rounds))
        .collect();

    let mut kills_per_round: HashMap<(usize, i64), usize> = HashMap::new();
    for k in kills {
        let n = k.round_number;
        let v = &mut stats[k.victim_index];
        v.row.deaths += 1;
        v.rounds_with_death.insert(n);

        let killer_idx = k.killer_index.filter(|&ki| ki != k.victim_index);
        if let Some(ki) = killer_idx {
            stats[k.victim_index].row.combat_death_count += 1;
            let kl = &mut stats[ki];
            kl.row.kills += 1;
            kl.rounds_with_kill.insert(n);
            if k.headshot {
                kl.row.headshot_count += 1;
            }
            if k.no_scope {
                kl.row.no_scope_kill_count += 1;
            }
            if k.penetrated_objects != 0 {
                kl.row.wallbang_kill_count += 1;
            }
            *kills_per_round.entry((ki, n)).or_insert(0) += 1;
        } else {
            stats[k.victim_index].row.bomb_death_count += 1;
        }

        if k.trade_kill {
            if let Some(ki) = k.killer_index {
                stats[ki].row.trade_kill_count += 1;
            }
        }
        if k.trade_death {
            let v = &mut stats[k.victim_index];
            v.row.trade_death_count += 1;
            v.rounds_traded.insert(n);
        }
        if let Some(ai) = k.assister_index {
            let a = &mut stats[ai];
            a.row.assists += 1;
            a.rounds_with_assist.insert(n);
        }
        if k.flash_assist {
            if let Some(fai) = k.flash_assister_index {
                stats[fai].row.flash_assist_count += 1;
            }
        }
    }

    let mut collateral_groups: HashMap<(usize, i64), usize> = HashMap::new();
    for k in kills {
        let Some(ki) = k.killer_index else {
            continue;
        };
        if ki == k.victim_index {
            continue;
        }
        *collateral_groups.entry((ki, k.tick)).or_insert(0) += 1;
    }
    for ((ki, _), cnt) in collateral_groups {
        if cnt >= 2 {
            stats[ki].row.collateral_kill_count += cnt;
        }
    }

    for r in bomb_events {
        if !matches!(r.kind, BombEventKind::Planted | BombEventKind::Defused) {
            continue;
        }
        if round_model
            .round_for_event(r.tick, r.total_rounds_played)
            .is_some_and(|n| round_model.has_round(n))
        {
            if let Some(idx) = players.idx(r.user_steamid) {
                match r.kind {
                    BombEventKind::Planted => stats[idx].row.bomb_plant_count += 1,
                    BombEventKind::Defused => stats[idx].row.bomb_defuse_count += 1,
                    _ => {}
                }
            }
        }
    }

    let mut sorted_kills: Vec<&KillRow> = kills.iter().collect();
    sorted_kills.sort_by_key(|k| k.tick);
    let mut first_kills: HashMap<i64, usize> = HashMap::new();
    let mut first_deaths: HashMap<i64, usize> = HashMap::new();
    for k in sorted_kills {
        if let Some(ki) = k.killer_index {
            first_kills.entry(k.round_number).or_insert(ki);
        }
        first_deaths.entry(k.round_number).or_insert(k.victim_index);
    }
    for idx in first_kills.into_values() {
        stats[idx].row.first_kill_count += 1;
    }
    for idx in first_deaths.into_values() {
        stats[idx].row.first_death_count += 1;
    }

    for ((ki, _), count) in kills_per_round {
        let s = &mut stats[ki].row;
        match count {
            1 => s.one_kill_count += 1,
            2 => s.two_kill_count += 1,
            3 => s.three_kill_count += 1,
            4 => s.four_kill_count += 1,
            c if c >= 5 => s.five_kill_count += 1,
            _ => {}
        }
    }

    for d in damages {
        let Some(atk) = d.attacker_index else {
            continue;
        };
        if atk == d.victim_index {
            continue;
        }
        if players.team_of_index(atk) == players.team_of_index(d.victim_index) {
            continue;
        }
        let s = &mut stats[atk].row;
        s.damage_health += d.health_damage;
        s.damage_armor += d.armor_damage;
        if matches!(
            d.weapon.to_ascii_lowercase().as_str(),
            "hegrenade" | "inferno" | "molotov" | "incendiary"
        ) {
            s.utility_damage += d.health_damage;
        }
    }

    for b in blinds {
        if players.team_of_index(b.flasher_index) != players.team_of_index(b.flashed_index) {
            stats[b.flasher_index].row.enemy_flash_duration_seconds += b.duration_seconds;
        } else {
            stats[b.flasher_index].row.team_flash_duration_seconds += b.duration_seconds;
        }
    }

    let all_rounds: HashSet<i64> = rounds.iter().map(|r| r.round_number).collect();
    for s in &mut stats {
        let mut kast = HashSet::new();
        for &rn in &all_rounds {
            if s.rounds_with_kill.contains(&rn)
                || s.rounds_with_assist.contains(&rn)
                || !s.rounds_with_death.contains(&rn)
                || s.rounds_traded.contains(&rn)
            {
                kast.insert(rn);
            }
        }
        s.row.kast_rounds = kast.len();
    }

    for c in clutches {
        let s = &mut stats[c.clutcher_index].row;
        match c.opponent_count.min(5) {
            1 => {
                s.vs_one_count += 1;
                if c.won {
                    s.vs_one_won_count += 1;
                } else {
                    s.vs_one_lost_count += 1;
                }
            }
            2 => {
                s.vs_two_count += 1;
                if c.won {
                    s.vs_two_won_count += 1;
                } else {
                    s.vs_two_lost_count += 1;
                }
            }
            3 => {
                s.vs_three_count += 1;
                if c.won {
                    s.vs_three_won_count += 1;
                } else {
                    s.vs_three_lost_count += 1;
                }
            }
            4 => {
                s.vs_four_count += 1;
                if c.won {
                    s.vs_four_won_count += 1;
                } else {
                    s.vs_four_lost_count += 1;
                }
            }
            5 => {
                s.vs_five_count += 1;
                if c.won {
                    s.vs_five_won_count += 1;
                } else {
                    s.vs_five_lost_count += 1;
                }
            }
            _ => {}
        }
    }

    let denom = total_rounds.max(1) as f64;
    stats
        .into_iter()
        .map(|mut s| {
            s.row.adr = round_f64(s.row.damage_health as f64 / denom, 2);
            s.row.kast = round_f64((s.row.kast_rounds as f64 / denom) * 100.0, 3);
            s.row.average_utility_damage_per_round =
                round_f64(s.row.utility_damage as f64 / denom, 2);
            s.row.enemy_flash_duration_seconds = round_f64(s.row.enemy_flash_duration_seconds, 3);
            s.row.team_flash_duration_seconds = round_f64(s.row.team_flash_duration_seconds, 3);
            s.row
        })
        .collect()
}
