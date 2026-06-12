"""cs2df CLI — reference exporter & validator for cs2-demo-format v3.

Commands:
    cs2df export <demo.dem> [-o out.zip] [--research] [--sample-rate 8]
    cs2df export-batch <dir> [--research] [--sample-rate 8] [--workers N] [--fail-fast] [--descriptive]
    cs2df validate <export.zip> [--spec DIR] [--strict]
"""

from __future__ import annotations

import argparse
import io
import json
import os
import sys
import tempfile
import time
import zipfile
from concurrent.futures import ProcessPoolExecutor, as_completed
from datetime import datetime
from pathlib import Path


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="cs2df",
                                     description="cs2-demo-format v3 reference exporter & validator")
    sub = parser.add_subparsers(dest="command", required=True)

    p_exp = sub.add_parser("export", help="export a CS2 .dem to a v3 ZIP package")
    p_exp.add_argument("demo", help="path to the .dem file")
    p_exp.add_argument("-o", "--output", default=None,
                       help="output zip path (default: <demo>.zip next to the input)")
    p_exp.add_argument("--research", action="store_true",
                       help="also emit duels.json (full-tick combat windows)")
    p_exp.add_argument("--sample-rate", type=int, default=8,
                       help="replay stream sample rate in Hz (default 8)")
    p_exp.add_argument("--window-before", type=int, default=2000,
                       help="duel window extent before each anchor, ms (default 2000)")
    p_exp.add_argument("--window-after", type=int, default=1000,
                       help="duel window extent after each anchor, ms (default 1000)")
    p_exp.add_argument("-q", "--quiet", action="store_true", help="suppress progress output")

    p_batch = sub.add_parser("export-batch", help="batch-export all .dem files in a directory")
    p_batch.add_argument("directory", help="directory to scan for .dem files (non-recursive)")
    p_batch.add_argument("--research", action="store_true",
                         help="also emit duels.json")
    p_batch.add_argument("--sample-rate", type=int, default=8,
                         help="replay stream sample rate in Hz (default 8)")
    p_batch.add_argument("--window-before", type=int, default=2000,
                         help="duel window extent before each anchor, ms (default 2000)")
    p_batch.add_argument("--window-after", type=int, default=1000,
                         help="duel window extent after each anchor, ms (default 1000)")
    p_batch.add_argument("--workers", type=int, default=None,
                         help="parallel worker count (default: logical CPU count)")
    p_batch.add_argument("--fail-fast", action="store_true",
                         help="stop on first failure")
    p_batch.add_argument("--descriptive", action="store_true",
                         help="use descriptive filenames (date_map_teams_score.zip)")

    p_val = sub.add_parser("validate", help="validate a v3 ZIP package")
    p_val.add_argument("zip", help="path to the .zip file to validate")
    p_val.add_argument("--spec", default=None, help="path to the spec/ directory")
    p_val.add_argument("--strict", action="store_true", help="treat warnings as errors")

    args = parser.parse_args(argv)

    if args.command == "export":
        return _cmd_export(args)
    if args.command == "export-batch":
        return _cmd_export_batch(args)
    if args.command == "validate":
        return _cmd_validate(args)
    return 2


def _cmd_export(args) -> int:
    from .package import export_demo

    dem = Path(args.demo)
    if not dem.exists():
        print(f"ERROR: demo not found: {dem}", file=sys.stderr)
        return 1
    out = Path(args.output) if args.output else dem.with_suffix(".zip")

    t0 = time.perf_counter()
    progress = None
    if not args.quiet:
        def progress(stage: str, frac: float) -> None:
            print(f"  [{frac * 100:5.1f}%] {stage}")

    data = export_demo(str(dem), research=args.research,
                       sample_rate=args.sample_rate,
                       window_before_ms=args.window_before,
                       window_after_ms=args.window_after,
                       progress=progress)
    out.write_bytes(data)
    dt = time.perf_counter() - t0
    print(f"wrote {out} ({len(data) / 1e6:.2f} MB) in {dt:.1f}s")
    return 0


def _cmd_export_batch(args) -> int:
    directory = Path(args.directory)
    if not directory.is_dir():
        print(f"ERROR: not a directory: {directory}", file=sys.stderr)
        return 1

    demos = sorted(directory.glob("*.dem"))
    if not demos:
        print(f"ERROR: no .dem files found in {directory}", file=sys.stderr)
        return 1

    workers = args.workers if args.workers is not None else _default_workers()
    if workers < 1:
        print("ERROR: --workers must be >= 1", file=sys.stderr)
        return 1

    # Shared export args (picklable, no callbacks).
    export_kwargs = {
        "research": args.research,
        "sample_rate": args.sample_rate,
        "window_before_ms": args.window_before,
        "window_after_ms": args.window_after,
    }

    report: list[dict] = []
    t0 = time.perf_counter()
    with tempfile.TemporaryDirectory(prefix="cs2df-") as tmp:
        tmp_dir = Path(tmp)
        with ProcessPoolExecutor(max_workers=workers) as pool:
            futures = {
                pool.submit(_export_one_report, str(dem), str(tmp_dir),
                            export_kwargs, args.descriptive): dem
                for dem in demos
            }
            for future in as_completed(futures):
                dem = futures[future]
                row = future.result()
                report.append(row)
                if row["ok"]:
                    print(f"  ok  {dem.name} -> {Path(row['zip']).name}  "
                          f"{row['zipBytes'] / 1e6:.1f}MB  {row['durationSeconds']:.1f}s")
                else:
                    print(f"  FAIL {dem.name}: {row['error']}  ({row['durationSeconds']:.1f}s)")
                    if args.fail_fast:
                        for pending in futures:
                            pending.cancel()
                        dt = time.perf_counter() - t0
                        _write_batch_report(directory, report, dt)
                        return 1

        dt = time.perf_counter() - t0
        # Copy individual ZIPs out of temp dir next to their source demos.
        for row in report:
            if row["ok"]:
                src = tmp_dir / Path(row["zip"]).name
                if src.exists():
                    dest = directory / Path(row["zip"]).name
                    dest.write_bytes(src.read_bytes())
        _write_batch_report(directory, report, dt)

    ok = sum(1 for r in report if r["ok"])
    fail = sum(1 for r in report if not r["ok"])
    total_mb = sum(r["demoBytes"] for r in report) / 1e6
    print(f"\n{ok} ok, {fail} failed, {dt:.1f}s total, {total_mb:.1f} MB demo data")
    return 1 if fail else 0


def _cmd_validate(args) -> int:
    from .validate import validate_zip

    zip_path = Path(args.zip)
    if not zip_path.exists():
        print(f"ERROR: file not found: {zip_path}", file=sys.stderr)
        return 1
    spec_dir = _resolve_spec_dir(args.spec)
    if spec_dir is None:
        print("ERROR: spec directory not found; pass --spec", file=sys.stderr)
        return 1
    ok = validate_zip(zip_path, spec_dir, strict=args.strict)
    return 0 if ok else 1


def _resolve_spec_dir(arg: str | None) -> Path | None:
    if arg:
        p = Path(arg)
        return p if p.exists() else None
    # repo layout: python/src/cs2df/cli.py → ../../../spec
    repo_spec = Path(__file__).resolve().parents[3] / "spec"
    if repo_spec.exists():
        return repo_spec
    return None


# ── batch helpers (module-level for picklability with ProcessPoolExecutor) ──────

def _export_one_report(dem_path: str, tmp_dir: str, export_kwargs: dict,
                       descriptive: bool) -> dict:
    """Parse → build → package one demo; return a structured result row."""
    from .package import export_demo

    dem = Path(dem_path)
    started = time.perf_counter()
    try:
        data = export_demo(str(dem), progress=None, **export_kwargs)
        if descriptive:
            name = _build_descriptive_name(data, dem)
        else:
            name = f"{dem.stem}.zip"
        zip_path = Path(tmp_dir) / name
        zip_path.write_bytes(data)
        duration = time.perf_counter() - started
        return {
            "demo": str(dem),
            "zip": name,
            "ok": True,
            "error": None,
            "durationSeconds": round(duration, 3),
            "demoBytes": dem.stat().st_size,
            "zipBytes": len(data),
        }
    except Exception as exc:
        duration = time.perf_counter() - started
        return {
            "demo": str(dem),
            "zip": None,
            "ok": False,
            "error": str(exc),
            "durationSeconds": round(duration, 3),
            "demoBytes": dem.stat().st_size if dem.exists() else 0,
            "zipBytes": 0,
        }


def _build_descriptive_name(zip_bytes: bytes, dem: Path) -> str:
    """Build a descriptive filename from match metadata; fall back to stem."""
    try:
        with zipfile.ZipFile(io.BytesIO(zip_bytes)) as zf:
            manifest = json.loads(zf.read("manifest.json"))
            match = json.loads(zf.read(manifest["files"]["match"]))

        mtime = os.path.getmtime(str(dem))
        date = datetime.fromtimestamp(mtime).strftime("%Y-%m-%d")
        map_name = _sanitize(match.get("mapName", "unknown"))
        team_a = _sanitize((match.get("teamA") or {}).get("name") or "")
        team_b = _sanitize((match.get("teamB") or {}).get("name") or "")
        score_a = (match.get("teamA") or {}).get("score", 0)
        score_b = (match.get("teamB") or {}).get("score", 0)

        if team_a and team_b:
            stem = f"{date}_{map_name}_{team_a}-vs-{team_b}_{score_a}-{score_b}"
        else:
            stem = f"{date}_{map_name}_{score_a}-{score_b}_{dem.stem}"
        return f"{stem}.zip"
    except Exception:
        return f"{dem.stem}.zip"


def _sanitize(s: str) -> str:
    """Sanitize a string for safe filename use."""
    for ch in r' <>:"/\|?*':
        s = s.replace(ch, '_')
    while '__' in s:
        s = s.replace('__', '_')
    return s.strip('_')


def _write_batch_report(out_dir: Path, report: list[dict],
                        duration_seconds: float) -> None:
    """Write report.json with aggregate stats next to the exported ZIPs."""
    demo_bytes = sum(r["demoBytes"] for r in report)
    zip_bytes = sum(r["zipBytes"] for r in report)
    ok_count = sum(1 for r in report if r["ok"])
    fail_count = sum(1 for r in report if not r["ok"])

    payload = {
        "createdAt": datetime.now().isoformat(),
        "total": len(report),
        "ok": ok_count,
        "failed": fail_count,
        "durationSeconds": round(duration_seconds, 3),
        "demoBytes": demo_bytes,
        "zipBytes": zip_bytes,
        "demoMegabytesPerSecond": round(
            (demo_bytes / 1_000_000) / duration_seconds, 3
        ) if duration_seconds else 0.0,
        "zipMegabytesPerSecond": round(
            (zip_bytes / 1_000_000) / duration_seconds, 3
        ) if duration_seconds else 0.0,
        "compressionRatio": round(zip_bytes / demo_bytes, 4) if demo_bytes else None,
        "items": sorted(report, key=lambda r: r["demo"]),
    }
    report_path = out_dir / "report.json"
    report_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2))
    print(f"wrote {report_path}")


def _default_workers() -> int:
    count = getattr(os, "process_cpu_count", os.cpu_count)
    return max(1, count() if callable(count) else (count or 1))


if __name__ == "__main__":
    raise SystemExit(main())
