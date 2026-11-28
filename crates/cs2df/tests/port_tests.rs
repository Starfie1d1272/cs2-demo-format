use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use cs2df::batch::{
    aggregate_timings, build_descriptive_name, commit_unique_zip, group_demos_in_dir, run_batch,
    strip_part_suffix, BatchItemRow,
};
use cs2df::enums::{Side, TeamKey};
use cs2df::events::{
    annotate_trades, build_damages, build_kills, DeathRawRow, HurtRawRow, KillRow, PlayerDirectory,
    PlayerRow, Pos3,
};
use cs2df::merge::merge_raw;
use cs2df::package::{ExportOptions, MatchMeta};
use cs2df::parse::{build_sample_ticks, parse_demo, ParseOptions, RawDemo};
use cs2df::rounds::{build_rounds, RoundMarkerRow, RoundModel, RoundWindow};
use cs2df::streams::{build_replay, delta_encode, DuelColumns, ReplayColumns};
use cs2df::validate::{decode_delta, validate_dir_fixture};
use tempfile::NamedTempFile;

fn make_players() -> PlayerDirectory {
    let teams = [
        TeamKey::TeamA,
        TeamKey::TeamA,
        TeamKey::TeamB,
        TeamKey::TeamB,
        TeamKey::TeamA,
    ];
    PlayerDirectory::new(
        teams
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let num = 76561198000000000u64 + i as u64;
                PlayerRow {
                    steam_id64: num.to_string(),
                    name: i.to_string(),
                    team_key: *t,
                    steam_id_num: num,
                }
            })
            .collect(),
    )
}

fn make_kill(killer: Option<usize>, victim: usize, tick: i64, round_number: i64) -> KillRow {
    KillRow {
        round_number,
        tick,
        killer_index: killer,
        victim_index: victim,
        assister_index: None,
        flash_assister_index: None,
        weapon: "ak47".to_string(),
        killer_active_weapon: None,
        victim_active_weapon: None,
        headshot: false,
        flash_assist: false,
        trade_kill: false,
        trade_death: false,
        through_smoke: false,
        no_scope: false,
        penetrated_objects: 0,
        killer_position: None,
        victim_position: Pos3 { x: 0, y: 0, z: 0 },
    }
}

fn marker(tick: i64, trp: i64, winner: Option<&str>) -> RoundMarkerRow {
    RoundMarkerRow {
        tick,
        total_rounds_played: trp,
        winner: winner.map(str::to_string),
        reason_str: None,
        reason_num: None,
    }
}

fn empty_raw(tickrate: i64) -> RawDemo {
    let _ = ParseOptions::default();
    RawDemo {
        timings: BTreeMap::new(),
        header: HashMap::new(),
        tickrate,
        sample_rate: 8,
        match_start_tick: 1,
        team_a_name: None,
        team_b_name: None,
        player_info: Vec::new(),
        round_starts: Vec::new(),
        round_freeze_ends: Vec::new(),
        round_ends: Vec::new(),
        deaths: Vec::new(),
        hurts: Vec::new(),
        fires: Vec::new(),
        blinds: Vec::new(),
        bomb_events: Vec::new(),
        grenade_detonations: Vec::new(),
        inferno_expires: Vec::new(),
        smoke_expires: Vec::new(),
        grenade_throws: Vec::new(),
        grenade_trajectories: Vec::new(),
        replay_cols: ReplayColumns::default(),
        replay_ticks: Vec::new(),
        duel_cols: DuelColumns::default(),
        duel_windows: Vec::new(),
        round_side_samples: Vec::new(),
        economy_raw: Vec::new(),
        freeze_ticks: Vec::new(),
    }
}

// ── round_windows ────────────────────────────────────────────────────────────

#[test]
fn round_for_tick_assigns_post_round_tail_to_previous_round() {
    let model = RoundModel::new(
        vec![
            RoundWindow {
                round_number: 1,
                start_tick: 50,
                freeze_end_tick: 100,
                end_tick: 200,
                next_start_tick: None,
            },
            RoundWindow {
                round_number: 2,
                start_tick: 300,
                freeze_end_tick: 360,
                end_tick: 460,
                next_start_tick: None,
            },
        ],
        HashMap::new(),
    );
    assert_eq!(model.round_for_tick(200), Some(1));
    assert_eq!(model.round_for_tick(250), Some(1));
    assert_eq!(model.round_for_tick(299), Some(1));
    assert_eq!(model.round_for_tick(300), Some(2));
}

#[test]
fn kills_after_round_end_before_next_start_are_kept_in_previous_round() {
    let players = PlayerDirectory::new(vec![
        PlayerRow {
            steam_id64: "76561198000000001".to_string(),
            name: "attacker".to_string(),
            team_key: TeamKey::TeamA,
            steam_id_num: 76561198000000001,
        },
        PlayerRow {
            steam_id64: "76561198000000002".to_string(),
            name: "victim".to_string(),
            team_key: TeamKey::TeamB,
            steam_id_num: 76561198000000002,
        },
    ]);
    let mut side_map = HashMap::new();
    side_map.insert((1, TeamKey::TeamA), Side::T);
    side_map.insert((1, TeamKey::TeamB), Side::Ct);
    let model = RoundModel::new(
        vec![
            RoundWindow {
                round_number: 1,
                start_tick: 50,
                freeze_end_tick: 100,
                end_tick: 200,
                next_start_tick: None,
            },
            RoundWindow {
                round_number: 2,
                start_tick: 300,
                freeze_end_tick: 360,
                end_tick: 460,
                next_start_tick: None,
            },
        ],
        side_map,
    );
    let deaths = vec![DeathRawRow {
        tick: 250,
        total_rounds_played: 0,
        user_steamid: Some(76561198000000002),
        attacker_steamid: Some(76561198000000001),
        assister_steamid: None,
        weapon: Some("ak47".to_string()),
        attacker_active_weapon: None,
        user_active_weapon: None,
        headshot: false,
        assistedflash: false,
        thrusmoke: false,
        noscope: false,
        penetrated: 0,
        attacker_x: None,
        attacker_y: None,
        attacker_z: None,
        user_x: Some(1.0),
        user_y: Some(2.0),
        user_z: Some(3.0),
    }];
    let kills = build_kills(&deaths, &players, &model, 64);
    assert_eq!(kills.len(), 1);
    assert_eq!(kills[0].round_number, 1);
    assert_eq!(kills[0].tick, 250);
}

#[test]
fn round_restart_keeps_previous_result_and_excludes_abandoned_ticks() {
    let starts = vec![
        marker(10, 0, None),
        marker(100, 1, None),
        marker(160, 1, None),
    ];
    let freezes = vec![marker(20, 0, None), marker(170, 1, None)];
    let ends = vec![
        marker(90, 1, Some("ct")),
        marker(160, 1, None),
        marker(200, 2, Some("ct")),
    ];

    let (rounds, model) = build_rounds(&starts, &freezes, &ends, &[], &[], &HashMap::new());
    let nums: Vec<i64> = rounds.iter().map(|r| r.round_number).collect();
    assert_eq!(nums, vec![1, 2]);
    assert_eq!(
        (rounds[1].start_tick, rounds[1].freeze_end_tick),
        (160, 170)
    );
    assert_eq!(model.round_for_tick(99), Some(1));
    assert_eq!(model.round_for_tick(100), None);
    assert_eq!(model.round_for_tick(159), None);
    assert_eq!(model.round_for_tick(160), Some(2));

    let ticks = build_sample_ticks(&ends, &freezes, &starts, 1);
    assert!(ticks.iter().all(|&t| !(100..170).contains(&t)));
    assert!(ticks.contains(&99) && ticks.contains(&170));

    let players = PlayerDirectory::new(vec![
        PlayerRow {
            steam_id64: "76561198000000001".to_string(),
            name: "attacker".to_string(),
            team_key: TeamKey::TeamA,
            steam_id_num: 76561198000000001,
        },
        PlayerRow {
            steam_id64: "76561198000000002".to_string(),
            name: "victim".to_string(),
            team_key: TeamKey::TeamB,
            steam_id_num: 76561198000000002,
        },
    ]);
    let hurts: Vec<HurtRawRow> = [130, 160, 180]
        .into_iter()
        .map(|tick| HurtRawRow {
            tick,
            total_rounds_played: 0,
            user_steamid: Some(76561198000000002),
            attacker_steamid: Some(76561198000000001),
            weapon: Some("ak47".to_string()),
            hitgroup: None,
            dmg_health: 50,
            dmg_armor: 0,
            armor: 100,
            attacker_x: None,
            attacker_y: None,
            attacker_z: None,
            user_x: None,
            user_y: None,
            user_z: None,
        })
        .collect();
    let damages = build_damages(&hurts, &players, &model);
    let pairs: Vec<(i64, i64)> = damages
        .iter()
        .map(|d| (d.tick, d.victim_health_before))
        .collect();
    assert_eq!(pairs, vec![(180, 100)]);
}

#[test]
fn rollback_replaces_scored_attempt_and_discards_later_old_rounds() {
    let starts = vec![
        marker(10, 0, None),
        marker(100, 1, None),
        marker(210, 2, None),
        marker(260, 1, None),
    ];
    let freezes = vec![
        marker(20, 0, None),
        marker(110, 1, None),
        marker(220, 2, None),
        marker(270, 1, None),
    ];
    let ends = vec![
        marker(90, 1, Some("ct")),
        marker(200, 2, Some("t")),
        marker(250, 3, Some("t")),
        marker(260, 1, None),
        marker(300, 2, Some("ct")),
    ];
    let (rounds, model) = build_rounds(&starts, &freezes, &ends, &[], &[], &HashMap::new());
    let nums: Vec<i64> = rounds.iter().map(|r| r.round_number).collect();
    assert_eq!(nums, vec![1, 2]);
    assert_eq!(
        (rounds[1].start_tick, rounds[1].freeze_end_tick),
        (260, 270)
    );
    assert_eq!((rounds[1].end_tick, rounds[1].winner_side), (300, Side::Ct));
    assert_eq!(model.round_for_tick(150), None);
    assert_eq!(model.round_for_tick(230), None);
    assert_eq!(model.round_for_tick(270), Some(2));
}

// ── trades ───────────────────────────────────────────────────────────────────

#[test]
fn unsorted_events_recover_nearest_teammate_and_clear_stale_flags() {
    let players = make_players();
    let mut first = make_kill(Some(2), 0, 100, 1);
    first.trade_death = true;
    let second = make_kill(Some(2), 1, 110, 1);
    let trade = make_kill(Some(4), 2, 120, 1);
    let mut rows = vec![trade.clone(), first.clone(), second.clone()];
    annotate_trades(&mut rows, &players, 384);
    assert_eq!(
        rows.iter().map(|r| r.tick).collect::<Vec<_>>(),
        vec![100, 110, 120]
    );
    assert_eq!(
        rows.iter().map(|r| r.trade_death).collect::<Vec<_>>(),
        vec![false, true, false]
    );
    assert_eq!(
        rows.iter().map(|r| r.trade_kill).collect::<Vec<_>>(),
        vec![false, false, true]
    );
    annotate_trades(&mut rows, &players, 384);
    assert_eq!(
        rows.iter().map(|r| r.trade_death).collect::<Vec<_>>(),
        vec![false, true, false]
    );
}

#[test]
fn invalid_links_are_not_trades() {
    let players = make_players();
    let cases = vec![
        (make_kill(Some(2), 0, 100, 1), make_kill(Some(1), 2, 110, 2)),
        (make_kill(Some(2), 2, 100, 1), make_kill(Some(0), 2, 110, 1)),
        (make_kill(Some(2), 3, 100, 1), make_kill(Some(0), 2, 110, 1)),
        (make_kill(Some(2), 0, 100, 1), make_kill(Some(2), 2, 110, 1)),
        (make_kill(Some(2), 0, 100, 1), make_kill(Some(3), 2, 110, 1)),
        (make_kill(None, 0, 100, 1), make_kill(Some(1), 2, 110, 1)),
        (make_kill(Some(2), 0, 100, 1), make_kill(None, 2, 110, 1)),
        (make_kill(Some(2), 0, 100, 1), make_kill(Some(1), 2, 485, 1)),
        (
            make_kill(Some(2), 2, 101299, 12),
            make_kill(Some(0), 2, 18017, 2),
        ),
    ];
    for (prior, current) in cases {
        let mut rows = vec![prior, current];
        annotate_trades(&mut rows, &players, 384);
        assert!(rows.iter().all(|r| !r.trade_kill && !r.trade_death));
    }
}

#[test]
fn window_is_inclusive_and_same_tick_order_is_stable() {
    let players = make_players();
    for delta in [0, 384] {
        let mut rows = vec![
            make_kill(Some(2), 0, 100, 1),
            make_kill(Some(1), 2, 100 + delta, 1),
        ];
        annotate_trades(&mut rows, &players, 384);
        assert!(rows[0].trade_death && rows[1].trade_kill);
    }
}

#[test]
fn unrelated_events_do_not_hide_a_trade_within_window() {
    let players = make_players();
    let mut rows = vec![make_kill(Some(2), 0, 100, 1)];
    for i in 0..25 {
        rows.push(make_kill(None, 3, 110 + i, 1));
    }
    rows.push(make_kill(Some(1), 2, 200, 1));
    annotate_trades(&mut rows, &players, 384);
    assert!(rows.first().unwrap().trade_death && rows.last().unwrap().trade_kill);
}

#[test]
fn builder_uses_demo_tickrate_and_sorts_parser_rows() {
    let players = make_players();
    let mut side_map = HashMap::new();
    side_map.insert((1, TeamKey::TeamA), Side::T);
    side_map.insert((1, TeamKey::TeamB), Side::Ct);
    let model = RoundModel::new(
        vec![RoundWindow {
            round_number: 1,
            start_tick: 1,
            freeze_end_tick: 10,
            end_tick: 2000,
            next_start_tick: None,
        }],
        side_map,
    );
    let death = |killer: usize, victim: usize, tick: i64| DeathRawRow {
        tick,
        total_rounds_played: 0,
        user_steamid: Some(players.rows[victim].steam_id_num),
        attacker_steamid: Some(players.rows[killer].steam_id_num),
        assister_steamid: None,
        weapon: Some("ak47".to_string()),
        attacker_active_weapon: None,
        user_active_weapon: None,
        headshot: false,
        assistedflash: false,
        thrusmoke: false,
        noscope: false,
        penetrated: 0,
        attacker_x: None,
        attacker_y: None,
        attacker_z: None,
        user_x: None,
        user_y: None,
        user_z: None,
    };
    let rows = build_kills(&[death(1, 2, 868), death(2, 0, 100)], &players, &model, 128);
    assert_eq!(
        rows.iter().map(|r| r.tick).collect::<Vec<_>>(),
        vec![100, 868]
    );
    assert!(rows[0].trade_death && rows[1].trade_kill);
}

// ── merge ────────────────────────────────────────────────────────────────────

fn mk_hurt(tick: i64) -> HurtRawRow {
    HurtRawRow {
        tick,
        total_rounds_played: 0,
        user_steamid: None,
        attacker_steamid: None,
        weapon: None,
        hitgroup: None,
        dmg_health: 0,
        dmg_armor: 0,
        armor: 0,
        attacker_x: None,
        attacker_y: None,
        attacker_z: None,
        user_x: None,
        user_y: None,
        user_z: None,
    }
}

#[test]
fn merge_two_parts_keeps_seam_round_shifts_ticks_and_trims_residue() {
    let mut p0 = empty_raw(64);
    p0.round_ends = vec![marker(200, 1, Some("t")), marker(460, 2, Some("ct"))];
    p0.round_starts = vec![marker(50, 0, None), marker(300, 1, None)];
    p0.round_freeze_ends = vec![marker(100, 0, None), marker(360, 1, None)];
    p0.hurts = vec![mk_hurt(150), mk_hurt(400)];

    let mut p1 = empty_raw(64);
    p1.round_ends = vec![
        marker(1, 2, Some("ct")),
        marker(200, 3, Some("t")),
        marker(460, 4, Some("ct")),
    ];
    p1.round_starts = vec![marker(2, 1, None), marker(5, 2, None), marker(300, 3, None)];
    p1.round_freeze_ends = vec![
        marker(3, 1, None),
        marker(60, 2, None),
        marker(360, 3, None),
    ];
    p1.hurts = vec![mk_hurt(4), mk_hurt(120), mk_hurt(400)];

    let merged = merge_raw(vec![p0, p1]).unwrap();
    let end_rounds: Vec<i64> = merged
        .round_ends
        .iter()
        .map(|r| r.total_rounds_played)
        .collect();
    assert_eq!(end_rounds, vec![1, 2, 3, 4]);

    let end_ticks: Vec<i64> = merged.round_ends.iter().map(|r| r.tick).collect();
    let mut sorted_ticks = end_ticks.clone();
    sorted_ticks.sort_unstable();
    assert_eq!(end_ticks, sorted_ticks);
    assert!(end_ticks[1] < end_ticks[2]);
    assert_eq!(merged.hurts.len(), 4);
    assert!(merged
        .hurts
        .iter()
        .all(|h| h.tick <= 460 || h.tick >= end_ticks[1]));
}

#[test]
fn merge_three_parts_avoids_double_offset_and_sums_timings() {
    let mut p0 = empty_raw(64);
    p0.timings.insert("parse.total".to_string(), 1.25);
    p0.round_starts = vec![marker(50, 0, None)];
    p0.round_freeze_ends = vec![marker(100, 0, None)];
    p0.round_ends = vec![marker(500, 1, Some("t"))];

    let mut p1 = empty_raw(64);
    p1.timings.insert("parse.total".to_string(), 2.5);
    p1.round_starts = vec![marker(50, 1, None)];
    p1.round_freeze_ends = vec![marker(100, 1, None)];
    p1.round_ends = vec![marker(600, 2, Some("ct"))];

    let mut p2 = empty_raw(64);
    p2.timings.insert("parse.total".to_string(), 3.75);
    p2.round_starts = vec![marker(50, 2, None)];
    p2.round_freeze_ends = vec![marker(100, 2, None)];
    p2.round_ends = vec![marker(700, 3, Some("t"))];

    // Expected shifts (gap = 64 * 8 = 512):
    // p0: offset 0 -> max_tick 500
    // p1: offset 500 + 512 = 1012 -> p1.end = 1612
    // p2: offset 1612 + 512 = 2124 -> p2.end = 2824 (NOT 500+512 + 1612+512!)
    let merged = merge_raw(vec![p0, p1, p2]).unwrap();
    let end_ticks: Vec<i64> = merged.round_ends.iter().map(|r| r.tick).collect();
    assert_eq!(end_ticks, vec![500, 1612, 2824]);
    assert_eq!(merged.timings.get("parse.total").copied(), Some(7.5));
}

#[test]
fn single_part_passthrough() {
    let mut p0 = empty_raw(64);
    p0.round_ends = vec![marker(200, 1, Some("t"))];
    let merged = merge_raw(vec![p0]).unwrap();
    assert_eq!(merged.round_ends[0].tick, 200);
}

// ── replay boundaries ────────────────────────────────────────────────────────

fn push_replay_row(cols: &mut ReplayColumns, tick: i64, inv: &[&str]) {
    cols.ticks.push(tick);
    cols.steamids.push(Some(76561198000000001));
    cols.x.push(Some(tick as f32));
    cols.y.push(Some((tick + 1) as f32));
    cols.z.push(Some((tick + 2) as f32));
    cols.yaw.push(Some(90.0));
    cols.pitch.push(Some(0.0));
    cols.health.push(Some(100.0));
    cols.armor.push(Some(100.0));
    cols.flash_duration.push(Some(0.0));
    cols.balance.push(Some(800.0));
    cols.current_equip_value.push(Some(2700.0));
    cols.has_defuser.push(false);
    let wc = cols.weapon_dict.index(Some("ak47"));
    cols.weapon_codes.push(wc);
    let pc = cols.place_dict.index(Some("Middle"));
    cols.place_codes.push(pc);
    let inv_vec: Vec<String> = inv.iter().map(|s| (*s).to_string()).collect();
    cols.push_inventory(&inv_vec);
}

#[test]
fn replay_round_extends_to_sample_before_next_round_start() {
    let players = PlayerDirectory::new(vec![PlayerRow {
        steam_id64: "76561198000000001".to_string(),
        name: "p1".to_string(),
        team_key: TeamKey::TeamA,
        steam_id_num: 76561198000000001,
    }]);
    let model = RoundModel::new(
        vec![
            RoundWindow {
                round_number: 1,
                start_tick: 50,
                freeze_end_tick: 100,
                end_tick: 200,
                next_start_tick: None,
            },
            RoundWindow {
                round_number: 2,
                start_tick: 300,
                freeze_end_tick: 360,
                end_tick: 460,
                next_start_tick: None,
            },
        ],
        HashMap::new(),
    );
    let mut cols = ReplayColumns::default();
    for t in (100..360).step_by(16) {
        push_replay_row(&mut cols, t, &[]);
    }
    let replay = build_replay(&cols, &[], &players, &model, 128, 8).unwrap();
    let first = &replay.rounds[0];
    let last_tick = first.start_tick + (first.frame_count as i64 - 1) * first.tick_step;
    assert_eq!(last_tick, 292);
    assert!(last_tick < 300);
}

#[test]
fn replay_marks_round_start_bomb_carrier_from_inventory() {
    let players = PlayerDirectory::new(vec![PlayerRow {
        steam_id64: "76561198000000001".to_string(),
        name: "p1".to_string(),
        team_key: TeamKey::TeamA,
        steam_id_num: 76561198000000001,
    }]);
    let model = RoundModel::new(
        vec![
            RoundWindow {
                round_number: 1,
                start_tick: 50,
                freeze_end_tick: 100,
                end_tick: 200,
                next_start_tick: None,
            },
            RoundWindow {
                round_number: 2,
                start_tick: 300,
                freeze_end_tick: 360,
                end_tick: 460,
                next_start_tick: None,
            },
        ],
        HashMap::new(),
    );
    let mut cols = ReplayColumns::default();
    for t in (360..=460).step_by(16) {
        push_replay_row(&mut cols, t, &["C4 Explosive"]);
    }
    let replay = build_replay(&cols, &[], &players, &model, 128, 8).unwrap();
    let second = &replay.rounds[0];
    assert_eq!(second.round_number, 2);
    assert!(second.players[0].flags.iter().all(|&f| (f & 2) != 0));
}

#[test]
fn parse_replay_ticks_include_post_round_tail_before_next_start() {
    let ends = vec![marker(200, 1, Some("t")), marker(460, 2, Some("ct"))];
    let freezes = vec![marker(100, 0, None), marker(360, 1, None)];
    let starts = vec![marker(50, 0, None), marker(300, 1, None)];
    let ticks = build_sample_ticks(&ends, &freezes, &starts, 16);
    assert!(ticks.contains(&292));
    assert!(!ticks.contains(&300));
}

// ── batch / cli / validate ───────────────────────────────────────────────────

#[test]
fn truncated_demo_returns_err_and_batch_isolates_it_with_report() {
    let tmp = std::env::temp_dir().join(format!("cs2df_trunc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let bad_dem = tmp.join("truncated.dem");
    std::fs::write(&bad_dem, b"short").unwrap();

    let err = parse_demo(&bad_dem, &ParseOptions::default(), None).unwrap_err();
    assert!(err.to_string().contains("16 bytes"));

    let ok = run_batch(&tmp, &ExportOptions::default(), 2, false, false).unwrap();
    assert!(!ok);
    let report_text = std::fs::read_to_string(tmp.join("report.json")).unwrap();
    let report: serde_json::Value = serde_json::from_str(&report_text).unwrap();
    assert_eq!(report["total"], 1);
    assert_eq!(report["ok"], 0);
    assert_eq!(report["failed"], 1);
    assert_eq!(report["items"][0]["ok"], false);

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn descriptive_unique_allocation_never_overwrites_existing_stem_zip() {
    let tmp = std::env::temp_dir().join(format!("cs2df_desc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // Pre-existing <stem>.zip in output dir must remain untouched
    let stem_zip = tmp.join("match-one.zip");
    std::fs::write(&stem_zip, b"existing-stem-zip").unwrap();

    let meta = MatchMeta {
        map_name: "de_mirage".to_string(),
        team_a_name: Some("Team A".to_string()),
        team_a_score: 13,
        team_b_name: Some("Team B".to_string()),
        team_b_score: 11,
        timings_seconds: BTreeMap::new(),
        compress_level: 3,
    };
    let pref = build_descriptive_name(&meta, "match-one", "2026-06-12");
    let tmp_zip = NamedTempFile::new_in(&tmp).unwrap();
    let out_p = commit_unique_zip(tmp_zip, &tmp, &pref, "match-one").unwrap();
    assert_eq!(
        out_p.file_name().unwrap(),
        "2026-06-12_de_mirage_Team_A-vs-Team_B_13-11.zip"
    );
    assert_eq!(std::fs::read(&stem_zip).unwrap(), b"existing-stem-zip");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn batch_groups_split_gotv_parts_and_allocates_unique_names() {
    let tmp = std::env::temp_dir().join(format!("cs2df_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    std::fs::write(tmp.join("match-a-p2.dem"), b"p2").unwrap();
    std::fs::write(tmp.join("match-a-p1.dem"), b"p1").unwrap();
    std::fs::write(tmp.join("match-b.dem"), b"b").unwrap();

    assert_eq!(strip_part_suffix("match-a-p1"), ("match-a", Some(1)));
    assert_eq!(strip_part_suffix("match-b"), ("match-b", None));

    let jobs = group_demos_in_dir(&tmp).unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].logical_stem, "match-a");
    assert_eq!(jobs[0].parts.len(), 2);
    assert!(jobs[0].parts[0].ends_with("match-a-p1.dem"));
    assert!(jobs[0].parts[1].ends_with("match-a-p2.dem"));
    assert_eq!(jobs[1].logical_stem, "match-b");

    let first = commit_unique_zip(
        NamedTempFile::new_in(&tmp).unwrap(),
        &tmp,
        "2026-06-12_de_mirage_A-vs-B_13-11.zip",
        "m1",
    )
    .unwrap();
    let second = commit_unique_zip(
        NamedTempFile::new_in(&tmp).unwrap(),
        &tmp,
        "2026-06-12_de_mirage_A-vs-B_13-11.zip",
        "m2",
    )
    .unwrap();
    assert_eq!(
        first.file_name().unwrap(),
        "2026-06-12_de_mirage_A-vs-B_13-11.zip"
    );
    assert_eq!(
        second.file_name().unwrap(),
        "2026-06-12_de_mirage_A-vs-B_13-11_m2.zip"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn aggregate_timings_totals_and_averages() {
    let mut t1 = BTreeMap::new();
    t1.insert("parse.total".to_string(), 2.0);
    t1.insert("package.writeZip".to_string(), 0.4);
    let mut t2 = BTreeMap::new();
    t2.insert("parse.total".to_string(), 4.0);
    t2.insert("package.writeZip".to_string(), 0.8);

    let report = vec![
        BatchItemRow {
            demo: "a.dem".to_string(),
            zip: Some("a.zip".to_string()),
            ok: true,
            error: None,
            duration_seconds: 2.4,
            demo_bytes: 100,
            zip_bytes: 10,
            compress_level: Some(3),
            timings_seconds: Some(t1),
        },
        BatchItemRow {
            demo: "b.dem".to_string(),
            zip: Some("b.zip".to_string()),
            ok: true,
            error: None,
            duration_seconds: 4.8,
            demo_bytes: 200,
            zip_bytes: 20,
            compress_level: Some(3),
            timings_seconds: Some(t2),
        },
        BatchItemRow {
            demo: "c.dem".to_string(),
            zip: None,
            ok: false,
            error: Some("err".to_string()),
            duration_seconds: 0.1,
            demo_bytes: 50,
            zip_bytes: 0,
            compress_level: None,
            timings_seconds: None,
        },
    ];
    let agg = aggregate_timings(&report);
    assert_eq!(agg.count, 2);
    assert_eq!(agg.total["parse.total"], 6.0);
    assert_eq!(agg.average["parse.total"], 3.0);
    assert_eq!(agg.total["package.writeZip"], 1.2);
    assert_eq!(agg.average["package.writeZip"], 0.6);
}

#[test]
fn delta_roundtrip_and_strict_json_rejects_nan() {
    let raw = vec![100, 108, 108, 95];
    let enc = delta_encode(&raw);
    assert_eq!(enc, vec![100, 8, 0, -13]);
    assert_eq!(decode_delta(&enc), raw);

    assert!(serde_json::from_str::<serde_json::Value>(r#"{"a": NaN}"#).is_err());
    assert!(serde_json::from_str::<serde_json::Value>(r#"{"a": -Infinity}"#).is_err());
}

#[test]
fn v3_mid_fixture_validates_strict() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture = root.join("fixtures").join("v3-mid");
    assert!(validate_dir_fixture(&fixture, None, true).unwrap());
}
