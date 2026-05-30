#!/usr/bin/env python3
"""
cs2-demo-format validator — validate a CS2 demo export ZIP against the canonical JSON schemas.

Requirements:
    pip install jsonschema

Usage:
    python tools/validate.py export.zip
    python tools/validate.py export.zip --spec path/to/spec/
    python tools/validate.py export.zip --strict    # warnings become errors
"""

import argparse
import json
import re
import sys
import zipfile
from pathlib import Path


# Required file keys that must be present in every valid export
REQUIRED_KEYS = {"manifest", "match", "players", "rounds", "playerStats", "playerEconomies",
                 "kills", "damages", "blinds", "bombs", "grenades", "clutches"}

# Known-valid schemaVersion strings
KNOWN_SCHEMA_VERSIONS = {"cs2-demo-format/1.0", "rivalhub-demo-export/1"}

# Files where roundNumber=0 warmup rows should have been filtered by the exporter
WARMUP_FILTERED_KEYS = {"kills", "damages", "blinds", "bombs", "grenades",
                         "clutches", "playerEconomies", "rounds"}


def find_invalid_json_values(text: str) -> list[tuple[int, str]]:
    """
    Scan raw text for NaN / ±Infinity before JSON parsing.
    Returns list of (char_offset, matched_value) for every hit.
    """
    hits = []
    for m in re.finditer(r'\b(NaN|-?Infinity)\b', text):
        hits.append((m.start(), m.group()))
    return hits


def sanitize(text: str) -> str:
    """Replace bare NaN / ±Infinity with null (backward-compat for existing exports)."""
    text = re.sub(r'\bNaN\b', 'null', text)
    text = re.sub(r'\b-?Infinity\b', 'null', text)
    return text


def load_schemas(spec_dir: Path) -> dict:
    schemas = {}
    for f in sorted(spec_dir.glob("*.schema.json")):
        key = f.stem.replace(".schema", "")
        schemas[key] = json.loads(f.read_text())
    return schemas


def validate_zip(zip_path: Path, spec_dir: Path, strict: bool = False) -> bool:
    try:
        import jsonschema
    except ImportError:
        print("ERROR: jsonschema not installed. Run: pip install jsonschema", file=sys.stderr)
        sys.exit(1)

    print(f"Validating: {zip_path.name}\n")

    schemas = load_schemas(spec_dir)
    if not schemas:
        print(f"ERROR: No schema files found in {spec_dir}", file=sys.stderr)
        sys.exit(1)

    errors: list[str] = []
    warnings: list[str] = []

    def err(msg: str):   errors.append(msg);   print(f"  ✗ {msg}")
    def warn(msg: str):  warnings.append(msg); print(f"  ⚠ {msg}")
    def ok(msg: str):    print(f"  ✓ {msg}")
    def skip(msg: str):  print(f"  - {msg}")

    with zipfile.ZipFile(zip_path) as zf:
        zip_names = set(zf.namelist())

        # ── manifest ─────────────────────────────────────────────────────────
        if "manifest.json" not in zip_names:
            err("manifest.json: missing from ZIP")
            _finish(errors, warnings, strict)
            return False

        raw_manifest = zf.read("manifest.json").decode("utf-8")
        hits = find_invalid_json_values(raw_manifest)
        if hits:
            warn(f"manifest.json: contains {len(hits)} invalid JSON value(s): "
                 f"{', '.join(v for _, v in hits[:3])} — exporter should output null instead")

        try:
            manifest = json.loads(sanitize(raw_manifest))
        except json.JSONDecodeError as e:
            err(f"manifest.json: JSON parse error — {e}")
            _finish(errors, warnings, strict)
            return False

        schema_version = manifest.get("schemaVersion", "")
        if schema_version not in KNOWN_SCHEMA_VERSIONS:
            warn(f"manifest.json: unknown schemaVersion '{schema_version}' "
                 f"(expected one of: {', '.join(sorted(KNOWN_SCHEMA_VERSIONS))})")

        if "manifest" in schemas:
            try:
                jsonschema.validate(manifest, schemas["manifest"])
                ok(f"manifest.json  (map: {manifest.get('mapName', '?')}, "
                   f"schema: {schema_version})")
            except jsonschema.ValidationError as e:
                err(f"manifest.json: [{_path(e)}] {e.message}")
        else:
            ok(f"manifest.json  (no schema to validate against)")

        files_map: dict = manifest.get("files", {})

        # ── check required files declared in manifest ─────────────────────────
        for key in REQUIRED_KEYS - {"manifest"}:
            if key not in files_map:
                err(f"(missing) key '{key}' not declared in manifest.files — required file absent")

        # ── validate each declared file ───────────────────────────────────────
        for key, filename in files_map.items():
            schema = schemas.get(key)

            if filename not in zip_names:
                if key in REQUIRED_KEYS:
                    err(f"{filename}: declared in manifest but missing from ZIP")
                else:
                    skip(f"{filename}: not present in ZIP (optional)")
                continue

            raw = zf.read(filename).decode("utf-8")

            # ── NaN / Infinity detection ──────────────────────────────────────
            hits = find_invalid_json_values(raw)
            if hits:
                sample = ", ".join(f"'{v}'@{off}" for off, v in hits[:5])
                warn(f"{filename}: {len(hits)} invalid JSON value(s) ({sample}…) — "
                     f"exporter should sanitize NaN/Infinity → null at source")

            try:
                data = json.loads(sanitize(raw))
            except json.JSONDecodeError as e:
                err(f"{filename}: JSON parse error — {e}")
                continue

            # ── warmup row detection ──────────────────────────────────────────
            if key in WARMUP_FILTERED_KEYS and isinstance(data, list):
                warmup_rows = [r for r in data if isinstance(r, dict) and r.get("roundNumber") == 0]
                if warmup_rows:
                    warn(f"{filename}: {len(warmup_rows)} warmup row(s) with roundNumber=0 — "
                         f"exporter should filter these out")

            # ── schema validation ─────────────────────────────────────────────
            if schema is None:
                skip(f"{filename}: no schema for key '{key}'")
                continue

            try:
                jsonschema.validate(data, schema)
                count = len(data) if isinstance(data, list) else 1
                ok(f"{filename}  ({count} {'rows' if count != 1 else 'row'})")
            except jsonschema.ValidationError as e:
                err(f"{filename}: [{_path(e)}] {e.message}")

    print()
    return _finish(errors, warnings, strict)


def _path(e) -> str:
    return " → ".join(str(p) for p in e.absolute_path) or "(root)"


def _finish(errors: list, warnings: list, strict: bool) -> bool:
    effective_errors = len(errors) + (len(warnings) if strict else 0)
    if effective_errors:
        parts = []
        if errors:    parts.append(f"{len(errors)} error(s)")
        if warnings:  parts.append(f"{len(warnings)} warning(s)" + (" [strict]" if strict else ""))
        print(f"❌ FAIL — {', '.join(parts)}")
        return False
    elif warnings:
        print(f"⚠️  PASS with {len(warnings)} warning(s)  (run --strict to treat as errors)")
        return True
    else:
        print("✅ PASS")
        return True


def main():
    parser = argparse.ArgumentParser(description="Validate a CS2 demo export ZIP")
    parser.add_argument("zip", help="Path to the .zip file to validate")
    parser.add_argument(
        "--spec",
        default=None,
        help="Path to spec/ directory with *.schema.json files "
             "(default: spec/ relative to this script's parent)",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Treat warnings as errors (exit 1 if any warnings)",
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
