use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::enums::GrenadeType;
use crate::events::{
    build_blinds, build_bombs, build_clutches, build_damages, build_economies, build_grenades,
    build_kills, build_match, build_player_stats, build_players, round_f64,
};
use crate::merge::parse_and_merge;
use crate::parse::{parse_demo, ParseOptions, ProgressFn, RawDemo};
use crate::rounds::build_rounds;
use crate::streams::{build_duels, build_replay, build_shots};

pub const SCHEMA_VERSION: &str = "cs2-demo-format/3.0";
pub const EXPORTER_NAME: &str = "cs2df";
pub const EXPORTER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PARSER_NAME: &str = "demoparser";
pub const PARSER_VERSION: &str = "0.42.2+39942c7";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersion {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoRef {
    pub hash: Option<String>,
    pub source_file_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: String,
    pub exporter: ToolVersion,
    pub parser: ToolVersion,
    pub demo: DemoRef,
    pub map_name: String,
    pub tickrate: i64,
    pub exported_at: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub research: bool,
    pub sample_rate: i64,
    pub window_before_ms: i64,
    pub window_after_ms: i64,
    pub compress_level: u32,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            research: false,
            sample_rate: 8,
            window_before_ms: 2000,
            window_after_ms: 1000,
            compress_level: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchMeta {
    pub map_name: String,
    pub team_a_name: Option<String>,
    pub team_a_score: i64,
    pub team_b_name: Option<String>,
    pub team_b_score: i64,
    pub timings_seconds: BTreeMap<String, f64>,
    pub compress_level: u32,
}

fn sha256_file(path: &Path) -> Result<String> {
    let f = File::open(path)?;
    let mut reader = BufReader::with_capacity(1 << 20, f);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn export_demo_merged_to_file(
    dem_paths: &[&Path],
    out_zip: &Path,
    opts: &ExportOptions,
    progress: Option<ProgressFn<'_>>,
) -> Result<(u64, MatchMeta)> {
    let (bytes, _, meta) =
        export_demo_merged_with_target(dem_paths, opts, progress, |_| Ok(out_zip.to_path_buf()))?;
    Ok((bytes, meta))
}

pub fn export_demo_merged_with_target<F>(
    dem_paths: &[&Path],
    opts: &ExportOptions,
    progress: Option<ProgressFn<'_>>,
    choose_target: F,
) -> Result<(u64, PathBuf, MatchMeta)>
where
    F: FnOnce(&MatchMeta) -> Result<PathBuf>,
{
    let demo_hash = if dem_paths.len() == 1 {
        sha256_file(dem_paths[0]).ok()
    } else {
        let mut h = Sha256::new();
        let mut ok = true;
        for p in dem_paths {
            match sha256_file(p) {
                Ok(hex_str) => h.update(hex_str.as_bytes()),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            Some(hex::encode(h.finalize()))
        } else {
            None
        }
    };

    let parse_opts = ParseOptions {
        sample_rate: opts.sample_rate,
        research: opts.research,
        window_before_ms: opts.window_before_ms,
        window_after_ms: opts.window_after_ms,
    };

    let scaled_cb = |stage: &str, frac: f32| {
        if let Some(p) = progress {
            p(stage, frac * 0.9);
        }
    };
    let raw = if dem_paths.len() == 1 {
        parse_demo(
            dem_paths[0],
            &parse_opts,
            if progress.is_some() {
                Some(&scaled_cb)
            } else {
                None
            },
        )?
    } else {
        parse_and_merge(
            dem_paths,
            &parse_opts,
            if progress.is_some() {
                Some(&scaled_cb)
            } else {
                None
            },
        )?
    };

    if let Some(p) = progress {
        p("build package", 0.92);
    }

    build_package_streaming(
        &raw,
        dem_paths[0],
        demo_hash,
        opts.window_before_ms,
        opts.window_after_ms,
        opts.compress_level,
        choose_target,
    )
}

fn build_package_streaming<F>(
    raw: &RawDemo,
    dem_path: &Path,
    demo_hash: Option<String>,
    window_before_ms: i64,
    window_after_ms: i64,
    compress_level: u32,
    choose_target: F,
) -> Result<(u64, PathBuf, MatchMeta)>
where
    F: FnOnce(&MatchMeta) -> Result<PathBuf>,
{
    let mut timings = raw.timings.clone();
    let tickrate = if raw.tickrate > 0 { raw.tickrate } else { 64 };
    let sample_rate = if raw.sample_rate > 0 {
        raw.sample_rate
    } else {
        8
    };

    let st = Instant::now();
    let players = build_players(&raw.player_info);
    timings.insert(
        "package.players".to_string(),
        round_f64(st.elapsed().as_secs_f64(), 3),
    );

    let team_map = players.team_by_sid.clone();

    let st = Instant::now();
    let (mut rounds, round_model) = build_rounds(
        &raw.round_starts,
        &raw.round_freeze_ends,
        &raw.round_ends,
        &raw.round_side_samples,
        &raw.player_info,
        &team_map,
    );
    timings.insert(
        "package.rounds".to_string(),
        round_f64(st.elapsed().as_secs_f64(), 3),
    );

    let st = Instant::now();
    let kills = build_kills(&raw.deaths, &players, &round_model, tickrate);
    let grenades = build_grenades(
        &raw.grenade_throws,
        &raw.grenade_detonations,
        &raw.inferno_expires,
        &raw.smoke_expires,
        &players,
        &round_model,
    );
    let mut flash_lookup: HashMap<(i64, i64), String> = HashMap::new();
    for g in &grenades {
        if g.grenade == GrenadeType::Flashbang {
            if let Some(gid) = &g.grenade_id {
                flash_lookup.insert((g.round_number, g.effect_tick), gid.clone());
            }
        }
    }
    let blinds = build_blinds(&raw.blinds, &players, &round_model, &flash_lookup);
    let damages = build_damages(&raw.hurts, &players, &round_model);
    let clutches = build_clutches(&kills, &rounds, &players);
    let economies = build_economies(&raw.economy_raw, &players, &round_model, &mut rounds);
    let player_stats = build_player_stats(
        &raw.bomb_events,
        &players,
        &round_model,
        &rounds,
        &kills,
        &blinds,
        &damages,
        &clutches,
    );
    let bombs = build_bombs(&raw.bomb_events, &players, &round_model);
    let match_json = build_match(
        &raw.header,
        tickrate,
        raw.team_a_name.as_deref(),
        raw.team_b_name.as_deref(),
        &rounds,
    );
    timings.insert(
        "package.events".to_string(),
        round_f64(st.elapsed().as_secs_f64(), 3),
    );

    let st = Instant::now();
    let shots = build_shots(&raw.fires, &players, &round_model);
    timings.insert(
        "package.shots".to_string(),
        round_f64(st.elapsed().as_secs_f64(), 3),
    );

    let st = Instant::now();
    let replay = build_replay(
        &raw.replay_cols,
        &raw.grenade_trajectories,
        &players,
        &round_model,
        tickrate,
        sample_rate,
    );
    timings.insert(
        "package.replay".to_string(),
        round_f64(st.elapsed().as_secs_f64(), 3),
    );

    let st = Instant::now();
    let duels = build_duels(
        &raw.duel_cols,
        &raw.duel_windows,
        &players,
        &round_model,
        tickrate,
        &kills,
        &damages,
        window_before_ms,
        window_after_ms,
    );
    timings.insert(
        "package.duels".to_string(),
        round_f64(st.elapsed().as_secs_f64(), 3),
    );

    let mut files = BTreeMap::new();
    files.insert("match".to_string(), "match.json".to_string());
    files.insert("players".to_string(), "players.json".to_string());
    files.insert("rounds".to_string(), "rounds.json".to_string());
    files.insert("playerStats".to_string(), "player-stats.json".to_string());
    files.insert(
        "playerEconomies".to_string(),
        "player-economies.json".to_string(),
    );
    files.insert("kills".to_string(), "kills.json".to_string());
    files.insert("damages".to_string(), "damages.json".to_string());
    files.insert("blinds".to_string(), "blinds.json".to_string());
    files.insert("bombs".to_string(), "bombs.json".to_string());
    files.insert("grenades".to_string(), "grenades.json".to_string());
    files.insert("clutches".to_string(), "clutches.json".to_string());
    if shots.is_some() {
        files.insert("shots".to_string(), "shots.json".to_string());
    }
    if replay.is_some() {
        files.insert("replay".to_string(), "replay.json".to_string());
    }
    if duels.is_some() {
        files.insert("duels".to_string(), "duels.json".to_string());
    }

    let map_name = raw
        .header
        .get("map_name")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("unknown")
        .to_string();

    let manifest = Manifest {
        schema_version: SCHEMA_VERSION.to_string(),
        exporter: ToolVersion {
            name: EXPORTER_NAME.to_string(),
            version: EXPORTER_VERSION.to_string(),
        },
        parser: ToolVersion {
            name: PARSER_NAME.to_string(),
            version: PARSER_VERSION.to_string(),
        },
        demo: DemoRef {
            hash: demo_hash,
            source_file_name: dem_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned()),
        },
        map_name,
        tickrate,
        exported_at: Utc::now().to_rfc3339(),
        files,
    };

    let mut meta = MatchMeta {
        map_name: match_json.map_name.clone(),
        team_a_name: match_json.team_a.name.clone(),
        team_a_score: match_json.team_a.score,
        team_b_name: match_json.team_b.name.clone(),
        team_b_score: match_json.team_b.score,
        timings_seconds: timings,
        compress_level,
    };

    let out_zip = choose_target(&meta)?;
    let st = Instant::now();
    let parent = out_zip.parent().unwrap_or_else(|| Path::new("."));
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent)?;
    }
    let fname = out_zip
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export.zip".to_string());
    let tmp_path = parent.join(format!(".{fname}.tmp.{}", std::process::id()));

    let write_res = (|| -> Result<u64> {
        {
            let f = File::create(&tmp_path)
                .with_context(|| format!("failed to create temp zip: {}", tmp_path.display()))?;
            let bw = BufWriter::with_capacity(1 << 20, f);
            let mut zw = ZipWriter::new(bw);

            let method = if compress_level == 0 {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
            let mut options = SimpleFileOptions::default().compression_method(method);
            if compress_level > 0 {
                options = options.compression_level(Some(compress_level as i64));
            }

            zw.start_file("manifest.json", options)?;
            serde_json::to_writer(&mut zw, &manifest)?;

            zw.start_file("match.json", options)?;
            serde_json::to_writer(&mut zw, &match_json)?;

            zw.start_file("players.json", options)?;
            serde_json::to_writer(&mut zw, &players.rows)?;

            zw.start_file("rounds.json", options)?;
            serde_json::to_writer(&mut zw, &rounds)?;

            zw.start_file("player-stats.json", options)?;
            serde_json::to_writer(&mut zw, &player_stats)?;

            zw.start_file("player-economies.json", options)?;
            serde_json::to_writer(&mut zw, &economies)?;

            zw.start_file("kills.json", options)?;
            serde_json::to_writer(&mut zw, &kills)?;

            zw.start_file("damages.json", options)?;
            serde_json::to_writer(&mut zw, &damages)?;

            zw.start_file("blinds.json", options)?;
            serde_json::to_writer(&mut zw, &blinds)?;

            zw.start_file("bombs.json", options)?;
            serde_json::to_writer(&mut zw, &bombs)?;

            zw.start_file("grenades.json", options)?;
            serde_json::to_writer(&mut zw, &grenades)?;

            zw.start_file("clutches.json", options)?;
            serde_json::to_writer(&mut zw, &clutches)?;

            if let Some(s) = shots {
                zw.start_file("shots.json", options)?;
                serde_json::to_writer(&mut zw, &s)?;
            }
            if let Some(r) = replay {
                zw.start_file("replay.json", options)?;
                serde_json::to_writer(&mut zw, &r)?;
            }
            if let Some(d) = duels {
                zw.start_file("duels.json", options)?;
                serde_json::to_writer(&mut zw, &d)?;
            }

            let mut bw = zw.finish()?;
            bw.flush()?;
        }
        let size = std::fs::metadata(&tmp_path)?.len();
        std::fs::rename(&tmp_path, &out_zip).with_context(|| {
            format!(
                "failed to rename {} -> {}",
                tmp_path.display(),
                out_zip.display()
            )
        })?;
        Ok(size)
    })();

    if write_res.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    let zip_bytes = write_res?;
    meta.timings_seconds.insert(
        "package.writeZip".to_string(),
        round_f64(st.elapsed().as_secs_f64(), 3),
    );
    Ok((zip_bytes, out_zip, meta))
}
