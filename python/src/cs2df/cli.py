"""cs2df CLI — reference exporter & validator for cs2-demo-format v3.

Commands:
    cs2df export <demo.dem> [-o out.zip] [--research] [--sample-rate 8]
    cs2df export-batch <dir> [--research] [--sample-rate 8]
    cs2df validate <export.zip> [--spec DIR] [--strict]
"""

from __future__ import annotations

import argparse
import sys
import time
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
    from .package import export_demo

    directory = Path(args.directory)
    if not directory.is_dir():
        print(f"ERROR: not a directory: {directory}", file=sys.stderr)
        return 1

    demos = sorted(directory.glob("*.dem"))
    if not demos:
        print(f"ERROR: no .dem files found in {directory}", file=sys.stderr)
        return 1

    ok = 0
    fail = 0
    t0 = time.perf_counter()
    for dem in demos:
        t1 = time.perf_counter()
        try:
            data = export_demo(str(dem), research=args.research,
                               sample_rate=args.sample_rate,
                               window_before_ms=args.window_before,
                               window_after_ms=args.window_after)
            out = dem.with_suffix(".zip")
            out.write_bytes(data)
            dt = time.perf_counter() - t1
            print(f"  ok  {dem.name} -> {out.name}  {len(data)/1e6:.1f}MB  {dt:.1f}s")
            ok += 1
        except Exception as exc:
            dt = time.perf_counter() - t1
            print(f"  FAIL {dem.name}: {exc}  ({dt:.1f}s)")
            fail += 1
    dt = time.perf_counter() - t0
    print(f"\n{ok} ok, {fail} failed, {dt:.1f}s total")
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


if __name__ == "__main__":
    raise SystemExit(main())
