use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{bail, Result};

use crate::events::round_f64;
use crate::parse::{parse_demo, ParseOptions, ProgressFn, RawDemo};

const GAP_SECONDS: i64 = 8;

fn round_of(total_rounds_played: i64, is_end: bool) -> i64 {
    if is_end {
        total_rounds_played
    } else {
        total_rounds_played + 1
    }
}

fn max_tick(raw: &RawDemo) -> i64 {
    let mut best = 0i64;
    for r in &raw.round_ends {
        if r.tick > best {
            best = r.tick;
        }
    }
    for r in &raw.round_freeze_ends {
        if r.tick > best {
            best = r.tick;
        }
    }
    for r in &raw.grenade_throws {
        if r.tick > best {
            best = r.tick;
        }
        if r.destroy_tick > best {
            best = r.destroy_tick;
        }
    }
    for &t in &raw.replay_ticks {
        if t > best {
            best = t;
        }
    }
    for &t in &raw.freeze_ticks {
        if t > best {
            best = t;
        }
    }
    best = best
        .max(raw.replay_cols.max_tick())
        .max(raw.duel_cols.max_tick());
    best
}

fn shift_tick(t: &mut i64, off: i64) {
    if *t > 0 {
        *t += off;
    }
}

fn shift_part(raw: &mut RawDemo, off: i64) {
    if off == 0 {
        return;
    }
    for r in &mut raw.round_starts {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.round_freeze_ends {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.round_ends {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.deaths {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.hurts {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.fires {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.blinds {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.bomb_events {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.grenade_detonations {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.inferno_expires {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.smoke_expires {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.grenade_throws {
        shift_tick(&mut r.tick, off);
        shift_tick(&mut r.destroy_tick, off);
    }
    for r in &mut raw.grenade_trajectories {
        shift_tick(&mut r.start_tick, off);
    }
    for r in &mut raw.round_side_samples {
        shift_tick(&mut r.tick, off);
    }
    for r in &mut raw.economy_raw {
        shift_tick(&mut r.tick, off);
    }
    raw.replay_cols.shift_ticks(off);
    raw.duel_cols.shift_ticks(off);
    for t in &mut raw.replay_ticks {
        *t += off;
    }
    for t in &mut raw.freeze_ticks {
        *t += off;
    }
    for (s, e) in &mut raw.duel_windows {
        *s += off;
        *e += off;
    }
}

pub fn merge_raw(mut parts: Vec<RawDemo>) -> Result<RawDemo> {
    if parts.is_empty() {
        bail!("merge_raw: no parts");
    }
    if parts.len() == 1 {
        return Ok(parts.pop().unwrap());
    }

    let mut running = 0i64;
    for part in &mut parts {
        shift_part(part, running);
        let tr = if part.tickrate > 0 { part.tickrate } else { 64 };
        running = max_tick(part) + tr * GAP_SECONDS;
    }

    let mut prev_max = 0i64;
    for (idx, part) in parts.iter_mut().enumerate() {
        if idx > 0 {
            part.round_ends
                .retain(|r| round_of(r.total_rounds_played, true) > prev_max);
            part.round_starts
                .retain(|r| round_of(r.total_rounds_played, false) > prev_max);
            part.round_freeze_ends
                .retain(|r| round_of(r.total_rounds_played, false) > prev_max);

            let trim_before = part
                .round_freeze_ends
                .iter()
                .filter(|r| r.tick > 0)
                .map(|r| r.tick)
                .min()
                .unwrap_or(0);
            if trim_before > 0 {
                let keep = |t: i64| !(t > 0 && t < trim_before);
                part.deaths.retain(|r| keep(r.tick));
                part.hurts.retain(|r| keep(r.tick));
                part.fires.retain(|r| keep(r.tick));
                part.blinds.retain(|r| keep(r.tick));
                part.bomb_events.retain(|r| keep(r.tick));
                part.grenade_detonations.retain(|r| keep(r.tick));
                part.inferno_expires.retain(|r| keep(r.tick));
                part.smoke_expires.retain(|r| keep(r.tick));
                part.grenade_throws.retain(|r| keep(r.tick));
                part.grenade_trajectories.retain(|r| keep(r.start_tick));
                part.round_side_samples.retain(|r| keep(r.tick));
                part.economy_raw.retain(|r| keep(r.tick));
            }
        }
        for r in &part.round_ends {
            let rn = round_of(r.total_rounds_played, true);
            if rn > prev_max {
                prev_max = rn;
            }
        }
    }

    let mut iter = parts.into_iter();
    let mut merged = iter.next().unwrap();
    let mut replay_tick_set: BTreeSet<i64> = merged.replay_ticks.iter().copied().collect();
    let mut freeze_tick_set: BTreeSet<i64> = merged.freeze_ticks.iter().copied().collect();

    for p in iter {
        for (k, v) in p.timings {
            let entry = merged.timings.entry(k).or_insert(0.0);
            *entry = round_f64(*entry + v, 3);
        }
        merged.round_starts.extend(p.round_starts);
        merged.round_freeze_ends.extend(p.round_freeze_ends);
        merged.round_ends.extend(p.round_ends);
        merged.deaths.extend(p.deaths);
        merged.hurts.extend(p.hurts);
        merged.fires.extend(p.fires);
        merged.blinds.extend(p.blinds);
        merged.bomb_events.extend(p.bomb_events);
        merged.grenade_detonations.extend(p.grenade_detonations);
        merged.inferno_expires.extend(p.inferno_expires);
        merged.smoke_expires.extend(p.smoke_expires);
        merged.grenade_throws.extend(p.grenade_throws);
        merged.grenade_trajectories.extend(p.grenade_trajectories);
        merged.replay_cols.extend_from(p.replay_cols);
        merged.duel_cols.extend_from(p.duel_cols);
        merged.duel_windows.extend(p.duel_windows);
        merged.round_side_samples.extend(p.round_side_samples);
        merged.economy_raw.extend(p.economy_raw);
        replay_tick_set.extend(p.replay_ticks);
        freeze_tick_set.extend(p.freeze_ticks);
    }

    merged.replay_ticks = replay_tick_set.into_iter().collect();
    merged.freeze_ticks = freeze_tick_set.into_iter().collect();
    Ok(merged)
}

pub fn parse_and_merge(
    dem_paths: &[&Path],
    opts: &ParseOptions,
    progress: Option<ProgressFn<'_>>,
) -> Result<RawDemo> {
    let n = dem_paths.len();
    let mut parts = Vec::with_capacity(n);
    for (i, path) in dem_paths.iter().enumerate() {
        let cb = |stage: &str, frac: f32| {
            if let Some(p) = progress {
                p(
                    &format!("[part {}/{n}] {stage}", i + 1),
                    (i as f32 + frac) / n as f32,
                );
            }
        };
        let raw = parse_demo(
            path,
            opts,
            if progress.is_some() { Some(&cb) } else { None },
        )?;
        parts.push(raw);
    }
    merge_raw(parts)
}
