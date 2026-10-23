"""Check that generated JSON Schemas accept the canonical wire payloads."""

import json
from pathlib import Path

from jsonschema import Draft7Validator


ROOT = Path(__file__).resolve().parents[2]


def test_v3_fixture_matches_generated_input_contract():
    fixture = ROOT / "fixtures" / "v3-mid"
    manifest = json.loads((fixture / "manifest.json").read_text())
    for key, filename in {"manifest": "manifest.json", **manifest["files"]}.items():
        schema = json.loads((ROOT / "spec" / f"{key}.schema.json").read_text())
        Draft7Validator.check_schema(schema)
        payload = json.loads((fixture / filename).read_text())
        errors = list(Draft7Validator(schema).iter_errors(payload))
        assert not errors, f"{filename}: {errors[0] if errors else ''}"
