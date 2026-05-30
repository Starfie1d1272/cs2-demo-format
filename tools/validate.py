#!/usr/bin/env python3
"""
cs2-demo-format validator — validate a CS2 demo export ZIP against the canonical JSON schemas.

Requirements:
    pip install jsonschema

Usage:
    python tools/validate.py export.zip
    python tools/validate.py export.zip --spec path/to/spec/   # custom spec dir
    python tools/validate.py export.zip --strict               # treat warnings as errors
"""

import argparse
import json
import re
import sys
import zipfile
from pathlib import Path


def sanitize(text: str) -> str:
    """Replace bare NaN / ±Infinity with null before JSON parsing."""
    text = re.sub(r'\bNaN\b', 'null', text)
    text = re.sub(r'\b-?Infinity\b', 'null', text)
    return text


def load_schemas(spec_dir: Path) -> dict:
    schemas = {}
    for f in spec_dir.glob("*.schema.json"):
        key = f.stem.replace(".schema", "")
        schemas[key] = json.loads(f.read_text())
    return schemas


def validate_zip(zip_path: Path, spec_dir: Path, strict: bool = False) -> bool:
    try:
        import jsonschema
    except ImportError:
        print("ERROR: jsonschema not installed. Run: pip install jsonschema", file=sys.stderr)
        sys.exit(1)

    print(f"Validating: {zip_path.name}")

    schemas = load_schemas(spec_dir)
    if not schemas:
        print(f"ERROR: No schema files found in {spec_dir}", file=sys.stderr)
        sys.exit(1)

    with zipfile.ZipFile(zip_path) as zf:
        # Parse manifest first
        try:
            manifest_text = sanitize(zf.read("manifest.json").decode("utf-8"))
            manifest = json.loads(manifest_text)
        except KeyError:
            print("  ✗ manifest.json: missing from ZIP")
            return False

        if "manifest" in schemas:
            try:
                jsonschema.validate(manifest, schemas["manifest"])
                print(f"  ✓ manifest.json  (map: {manifest.get('mapName', '?')})")
            except jsonschema.ValidationError as e:
                print(f"  ✗ manifest.json: {e.message}")
                return False

        # Validate each file declared in manifest.files
        errors = 0
        warnings = 0
        files_map = manifest.get("files", {})

        for key, filename in files_map.items():
            schema = schemas.get(key)
            if schema is None:
                msg = f"  ? {filename}: no schema for key '{key}' (unknown file)"
                if strict:
                    print(msg.replace("?", "✗"))
                    errors += 1
                else:
                    print(msg + " [skip]")
                    warnings += 1
                continue

            try:
                raw_text = zf.read(filename).decode("utf-8")
            except KeyError:
                # Optional files (shots, positions-1s) may be absent
                print(f"  - {filename}: not present in ZIP [skip]")
                warnings += 1
                continue

            try:
                data = json.loads(sanitize(raw_text))
            except json.JSONDecodeError as e:
                print(f"  ✗ {filename}: JSON parse error — {e}")
                errors += 1
                continue

            try:
                jsonschema.validate(data, schema)
                count = len(data) if isinstance(data, list) else 1
                print(f"  ✓ {filename}  ({count} {'rows' if count != 1 else 'row'})")
            except jsonschema.ValidationError as e:
                path = " → ".join(str(p) for p in e.absolute_path) or "(root)"
                print(f"  ✗ {filename}: [{path}] {e.message}")
                errors += 1

    print()
    if errors:
        print(f"❌ FAIL — {errors} error(s), {warnings} warning(s)")
        return False
    else:
        status = "⚠️  PASS with warnings" if warnings else "✅ PASS"
        print(f"{status} — {warnings} warning(s)" if warnings else f"{status}")
        return True


def main():
    parser = argparse.ArgumentParser(description="Validate a CS2 demo export ZIP")
    parser.add_argument("zip", help="Path to the .zip file to validate")
    parser.add_argument(
        "--spec",
        default=None,
        help="Path to spec/ directory containing *.schema.json files "
             "(default: spec/ relative to this script)",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Treat unknown file keys as errors instead of warnings",
    )
    args = parser.parse_args()

    zip_path = Path(args.zip)
    if not zip_path.exists():
        print(f"ERROR: file not found: {zip_path}", file=sys.stderr)
        sys.exit(1)

    spec_dir = Path(args.spec) if args.spec else Path(__file__).parent.parent / "spec"
    if not spec_dir.exists():
        print(f"ERROR: spec directory not found: {spec_dir}", file=sys.stderr)
        sys.exit(1)

    ok = validate_zip(zip_path, spec_dir, strict=args.strict)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
