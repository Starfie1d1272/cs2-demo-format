use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};

use cs2df::batch::{run_batch, strip_part_suffix};
use cs2df::package::{export_demo_merged_to_file, ExportOptions};
use cs2df::validate::validate_zip;

#[derive(Parser)]
#[command(
    name = "cs2df",
    version,
    about = "cs2-demo-format v3 exporter & validator"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Export one or more .dem files (split GOTV parts) to a v3 ZIP package
    Export {
        /// Path(s) to the .dem file; pass multiple split parts (…-p1.dem …-p2.dem) of ONE match to merge them
        #[arg(required = true)]
        demo: Vec<PathBuf>,

        /// Output zip path (default: <demo>.zip next to the input)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Also emit duels.json (full-tick combat windows)
        #[arg(long)]
        research: bool,

        /// Replay stream sample rate in Hz
        #[arg(long, default_value_t = 8)]
        sample_rate: i64,

        /// Duel window extent before each anchor, ms
        #[arg(long, default_value_t = 2000)]
        window_before: i64,

        /// Duel window extent after each anchor, ms
        #[arg(long, default_value_t = 1000)]
        window_after: i64,

        /// ZIP DEFLATE compression level 0-9
        #[arg(long, default_value_t = 3)]
        compress_level: u32,

        /// Suppress progress output
        #[arg(short, long)]
        quiet: bool,
    },

    /// Batch-export all .dem files in a directory
    ExportBatch {
        /// Directory to scan for .dem files (non-recursive)
        directory: PathBuf,

        /// Also emit duels.json
        #[arg(long)]
        research: bool,

        /// Replay stream sample rate in Hz
        #[arg(long, default_value_t = 8)]
        sample_rate: i64,

        /// Duel window extent before each anchor, ms
        #[arg(long, default_value_t = 2000)]
        window_before: i64,

        /// Duel window extent after each anchor, ms
        #[arg(long, default_value_t = 1000)]
        window_after: i64,

        /// ZIP DEFLATE compression level 0-9
        #[arg(long, default_value_t = 3)]
        compress_level: u32,

        /// Parallel worker count (default: logical CPU count)
        #[arg(long)]
        workers: Option<usize>,

        /// Stop on first failure
        #[arg(long)]
        fail_fast: bool,

        /// Use descriptive filenames (date_map_teams_score.zip)
        #[arg(long)]
        descriptive: bool,
    },

    /// Validate a v3 ZIP package
    Validate {
        /// Path to the .zip file to validate
        zip: PathBuf,

        /// Optional path to external spec/ directory (defaults to embedded schemas)
        #[arg(long)]
        spec: Option<PathBuf>,

        /// Treat warnings as errors
        #[arg(long)]
        strict: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Export {
            demo,
            output,
            research,
            sample_rate,
            window_before,
            window_after,
            compress_level,
            quiet,
        } => {
            for d in &demo {
                if !d.exists() {
                    eprintln!("ERROR: demo not found: {}", d.display());
                    return ExitCode::from(1);
                }
            }
            if compress_level > 9 {
                eprintln!("ERROR: --compress-level must be between 0 and 9");
                return ExitCode::from(1);
            }
            let first = &demo[0];
            let first_stem = first
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let (default_stem, _) = strip_part_suffix(first_stem);
            let out = output.unwrap_or_else(|| first.with_file_name(format!("{default_stem}.zip")));

            let t0 = Instant::now();
            if demo.len() > 1 {
                println!(
                    "合并 {} 段 → {}",
                    demo.len(),
                    out.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            let progress_cb = |stage: &str, frac: f32| {
                println!("  [{:5.1}%] {stage}", frac * 100.0);
            };
            let opts = ExportOptions {
                research,
                sample_rate,
                window_before_ms: window_before,
                window_after_ms: window_after,
                compress_level,
            };
            let dem_refs: Vec<&Path> = demo.iter().map(PathBuf::as_path).collect();
            match export_demo_merged_to_file(
                &dem_refs,
                &out,
                &opts,
                if quiet { None } else { Some(&progress_cb) },
            ) {
                Ok((bytes, _)) => {
                    let dt = t0.elapsed().as_secs_f64();
                    println!(
                        "wrote {} ({:.2} MB) in {dt:.1}s",
                        out.display(),
                        bytes as f64 / 1e6
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("ERROR: {e:#}");
                    ExitCode::from(1)
                }
            }
        }

        Commands::ExportBatch {
            directory,
            research,
            sample_rate,
            window_before,
            window_after,
            compress_level,
            workers,
            fail_fast,
            descriptive,
        } => {
            if !directory.is_dir() {
                eprintln!("ERROR: not a directory: {}", directory.display());
                return ExitCode::from(1);
            }
            if compress_level > 9 {
                eprintln!("ERROR: --compress-level must be between 0 and 9");
                return ExitCode::from(1);
            }
            let workers = workers.unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            });
            if workers < 1 {
                eprintln!("ERROR: --workers must be >= 1");
                return ExitCode::from(1);
            }
            let opts = ExportOptions {
                research,
                sample_rate,
                window_before_ms: window_before,
                window_after_ms: window_after,
                compress_level,
            };
            match run_batch(&directory, &opts, workers, fail_fast, descriptive) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(1),
                Err(e) => {
                    eprintln!("ERROR: {e:#}");
                    ExitCode::from(1)
                }
            }
        }

        Commands::Validate { zip, spec, strict } => {
            if !zip.exists() {
                eprintln!("ERROR: file not found: {}", zip.display());
                return ExitCode::from(1);
            }
            if let Some(ref s) = spec {
                if !s.exists() {
                    eprintln!("ERROR: spec directory not found: {}", s.display());
                    return ExitCode::from(1);
                }
            }
            match validate_zip(&zip, spec.as_deref(), strict) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(1),
                Err(e) => {
                    eprintln!("ERROR: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
