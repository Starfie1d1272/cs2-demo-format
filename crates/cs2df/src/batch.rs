use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Instant, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::Serialize;
use tempfile::NamedTempFile;

use crate::events::round_f64;
use crate::package::{export_demo_merged_with_target, ExportOptions, MatchMeta};

#[derive(Debug, Clone)]
pub struct LogicalMapJob {
    pub logical_stem: String,
    pub parts: Vec<PathBuf>,
}

pub fn strip_part_suffix(stem: &str) -> (&str, Option<u32>) {
    if let Some(pos) = stem.rfind("-p") {
        let suffix = &stem[pos + 2..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(num) = suffix.parse::<u32>() {
                return (&stem[..pos], Some(num));
            }
        }
    }
    (stem, None)
}

pub fn group_demos_in_dir(dir: &Path) -> Result<Vec<LogicalMapJob>> {
    let mut dem_files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("dem") {
            dem_files.push(path);
        }
    }
    dem_files.sort();

    let mut groups: BTreeMap<String, Vec<(u32, PathBuf)>> = BTreeMap::new();
    for path in dem_files {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("demo");
        let (base, part_num) = strip_part_suffix(stem);
        groups
            .entry(base.to_string())
            .or_default()
            .push((part_num.unwrap_or(0), path));
    }

    let mut jobs = Vec::with_capacity(groups.len());
    for (logical_stem, mut parts) in groups {
        parts.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        jobs.push(LogicalMapJob {
            logical_stem,
            parts: parts.into_iter().map(|(_, p)| p).collect(),
        });
    }
    Ok(jobs)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItemRow {
    pub demo: String,
    pub zip: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
    pub duration_seconds: f64,
    pub demo_bytes: u64,
    pub zip_bytes: u64,
    pub compress_level: Option<u32>,
    pub timings_seconds: Option<BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AggregateTimings {
    pub count: usize,
    pub total: BTreeMap<String, f64>,
    pub average: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReport {
    pub created_at: String,
    pub total: usize,
    pub ok: usize,
    pub failed: usize,
    pub duration_seconds: f64,
    pub demo_bytes: u64,
    pub zip_bytes: u64,
    pub demo_megabytes_per_second: f64,
    pub zip_megabytes_per_second: f64,
    pub compression_ratio: Option<f64>,
    pub timings_seconds: AggregateTimings,
    pub items: Vec<BatchItemRow>,
}

pub fn aggregate_timings(report: &[BatchItemRow]) -> AggregateTimings {
    let rows: Vec<&BTreeMap<String, f64>> = report
        .iter()
        .filter(|r| r.ok)
        .filter_map(|r| r.timings_seconds.as_ref())
        .collect();
    if rows.is_empty() {
        return AggregateTimings {
            count: 0,
            total: BTreeMap::new(),
            average: BTreeMap::new(),
        };
    }
    let mut keys = BTreeSet::new();
    for r in &rows {
        for k in r.keys() {
            keys.insert(k.clone());
        }
    }
    let mut total = BTreeMap::new();
    let mut average = BTreeMap::new();
    let count = rows.len();
    for k in keys {
        let sum: f64 = rows.iter().map(|r| r.get(&k).copied().unwrap_or(0.0)).sum();
        let rounded_sum = round_f64(sum, 3);
        total.insert(k.clone(), rounded_sum);
        average.insert(k, round_f64(rounded_sum / count as f64, 3));
    }
    AggregateTimings {
        count,
        total,
        average,
    }
}

pub fn sanitize_filename(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(
            ch,
            ' ' | '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
        ) {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

pub fn build_descriptive_name(meta: &MatchMeta, stem: &str, date_str: &str) -> String {
    let map_name = sanitize_filename(&meta.map_name);
    let team_a = meta
        .team_a_name
        .as_deref()
        .map(sanitize_filename)
        .unwrap_or_default();
    let team_b = meta
        .team_b_name
        .as_deref()
        .map(sanitize_filename)
        .unwrap_or_default();
    let score_a = meta.team_a_score;
    let score_b = meta.team_b_score;
    if !team_a.is_empty() && !team_b.is_empty() {
        format!("{date_str}_{map_name}_{team_a}-vs-{team_b}_{score_a}-{score_b}.zip")
    } else {
        format!("{date_str}_{map_name}_{score_a}-{score_b}_{stem}.zip")
    }
}

pub fn commit_unique_zip(
    mut tmp: NamedTempFile,
    out_dir: &Path,
    preferred_name: &str,
    stem: &str,
) -> Result<PathBuf> {
    let preferred = Path::new(preferred_name);
    let suffix = preferred
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_else(|| ".zip".to_string());
    let base = preferred
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let san = sanitize_filename(stem);
            if san.is_empty() {
                "export".to_string()
            } else {
                san
            }
        });
    let fallback = {
        let san = sanitize_filename(stem);
        if san.is_empty() {
            "export".to_string()
        } else {
            san
        }
    };

    let mut candidates = vec![
        format!("{base}{suffix}"),
        format!("{base}_{fallback}{suffix}"),
    ];
    for i in 2..1000 {
        candidates.push(format!("{base}_{fallback}_{i}{suffix}"));
    }

    for name in candidates {
        let path = out_dir.join(&name);
        match tmp.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
                tmp = e.file;
                continue;
            }
            Err(e) => {
                return Err(anyhow::Error::from(e.error))
                    .with_context(|| format!("failed to persist {}", path.display()));
            }
        }
    }
    anyhow::bail!("could not allocate unique output name for {preferred_name}");
}

fn file_date_str(path: &Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else {
        return "unknown".to_string();
    };
    let Ok(mtime) = meta.modified() else {
        return "unknown".to_string();
    };
    let Ok(dur) = mtime.duration_since(UNIX_EPOCH) else {
        return "unknown".to_string();
    };
    let dt: DateTime<Local> = (UNIX_EPOCH + dur).into();
    dt.format("%Y-%m-%d").to_string()
}

fn mb_per_s(total_bytes: u64, duration_seconds: f64) -> f64 {
    if duration_seconds <= 0.0 {
        return 0.0;
    }
    round_f64((total_bytes as f64 / 1_000_000.0) / duration_seconds, 3)
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        format!("panic: {s}")
    } else if let Some(s) = payload.downcast_ref::<String>() {
        format!("panic: {s}")
    } else {
        "panic: unknown payload".to_string()
    }
}

pub fn run_batch(
    directory: &Path,
    opts: &ExportOptions,
    workers: usize,
    fail_fast: bool,
    descriptive: bool,
) -> Result<bool> {
    let jobs = group_demos_in_dir(directory)?;
    if jobs.is_empty() {
        anyhow::bail!("no .dem files found in {}", directory.display());
    }

    let workers = workers.max(1).min(jobs.len());
    let queue = Mutex::new(VecDeque::from(jobs));
    let results = Mutex::new(Vec::new());
    let stop = AtomicBool::new(false);
    let unique_lock = Mutex::new(());
    let t0 = Instant::now();

    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                if fail_fast && stop.load(Ordering::Relaxed) {
                    break;
                }
                let job = {
                    let mut q = queue.lock().unwrap();
                    q.pop_front()
                };
                let Some(job) = job else {
                    break;
                };
                let first_dem = &job.parts[0];
                let demo_bytes: u64 = job
                    .parts
                    .iter()
                    .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
                    .sum();
                let started = Instant::now();
                let part_refs: Vec<&Path> = job.parts.iter().map(PathBuf::as_path).collect();

                let res = catch_unwind(AssertUnwindSafe(|| {
                    export_demo_merged_with_target(
                        &part_refs,
                        directory,
                        opts,
                        None,
                        |tmp, meta| {
                            if descriptive {
                                let date_str = file_date_str(first_dem);
                                let pref =
                                    build_descriptive_name(meta, &job.logical_stem, &date_str);
                                let _guard = unique_lock.lock().unwrap();
                                commit_unique_zip(tmp, directory, &pref, &job.logical_stem)
                            } else {
                                let path = directory.join(format!("{}.zip", job.logical_stem));
                                tmp.persist(&path)
                                    .map_err(|e| anyhow::Error::from(e.error))
                                    .with_context(|| {
                                        format!("failed to persist {}", path.display())
                                    })?;
                                Ok(path)
                            }
                        },
                    )
                }));

                let row = match res {
                    Ok(Ok((zip_bytes, final_zip_path, meta))) => {
                        let dur = started.elapsed().as_secs_f64();
                        let zip_name = final_zip_path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        println!(
                            "  ok  {} -> {}  {:.1}MB  {:.1}s",
                            first_dem.file_name().unwrap_or_default().to_string_lossy(),
                            zip_name,
                            zip_bytes as f64 / 1e6,
                            dur
                        );
                        BatchItemRow {
                            demo: first_dem.display().to_string(),
                            zip: Some(zip_name),
                            ok: true,
                            error: None,
                            duration_seconds: round_f64(dur, 3),
                            demo_bytes,
                            zip_bytes,
                            compress_level: Some(meta.compress_level),
                            timings_seconds: Some(meta.timings_seconds),
                        }
                    }
                    Ok(Err(e)) => {
                        let dur = started.elapsed().as_secs_f64();
                        println!(
                            "  FAIL {}: {}  ({:.1}s)",
                            first_dem.file_name().unwrap_or_default().to_string_lossy(),
                            e,
                            dur
                        );
                        if fail_fast {
                            stop.store(true, Ordering::Relaxed);
                        }
                        BatchItemRow {
                            demo: first_dem.display().to_string(),
                            zip: None,
                            ok: false,
                            error: Some(e.to_string()),
                            duration_seconds: round_f64(dur, 3),
                            demo_bytes,
                            zip_bytes: 0,
                            compress_level: None,
                            timings_seconds: None,
                        }
                    }
                    Err(panic_err) => {
                        let dur = started.elapsed().as_secs_f64();
                        let msg = panic_message(panic_err);
                        println!(
                            "  FAIL {}: {}  ({:.1}s)",
                            first_dem.file_name().unwrap_or_default().to_string_lossy(),
                            msg,
                            dur
                        );
                        if fail_fast {
                            stop.store(true, Ordering::Relaxed);
                        }
                        BatchItemRow {
                            demo: first_dem.display().to_string(),
                            zip: None,
                            ok: false,
                            error: Some(msg),
                            duration_seconds: round_f64(dur, 3),
                            demo_bytes,
                            zip_bytes: 0,
                            compress_level: None,
                            timings_seconds: None,
                        }
                    }
                };
                results.lock().unwrap().push(row);
            });
        }
    });

    let dt = t0.elapsed().as_secs_f64();
    let mut items = results.into_inner().unwrap();
    items.sort_by(|a, b| a.demo.cmp(&b.demo));

    let demo_bytes: u64 = items.iter().map(|r| r.demo_bytes).sum();
    let zip_bytes: u64 = items.iter().map(|r| r.zip_bytes).sum();
    let ok_count = items.iter().filter(|r| r.ok).count();
    let fail_count = items.len() - ok_count;
    let timings_agg = aggregate_timings(&items);

    let report = BatchReport {
        created_at: Local::now().to_rfc3339(),
        total: items.len(),
        ok: ok_count,
        failed: fail_count,
        duration_seconds: round_f64(dt, 3),
        demo_bytes,
        zip_bytes,
        demo_megabytes_per_second: mb_per_s(demo_bytes, dt),
        zip_megabytes_per_second: mb_per_s(zip_bytes, dt),
        compression_ratio: if demo_bytes > 0 {
            Some(round_f64(zip_bytes as f64 / demo_bytes as f64, 4))
        } else {
            None
        },
        timings_seconds: timings_agg,
        items,
    };

    let report_path = directory.join("report.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    println!("wrote {}", report_path.display());
    println!(
        "
{ok_count} ok, {fail_count} failed, {dt:.1}s total, {:.1} MB demo data",
        demo_bytes as f64 / 1e6
    );

    Ok(fail_count == 0)
}
