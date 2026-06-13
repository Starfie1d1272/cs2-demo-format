#!/usr/bin/env python3
"""Check release version fields that must move together."""

from __future__ import annotations

import ast
import json
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def _module_dunder_version(path: Path) -> str:
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        if not any(isinstance(target, ast.Name) and target.id == "__version__" for target in node.targets):
            continue
        if isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
            return node.value.value
    raise RuntimeError(f"{path}: missing string __version__")


def main() -> int:
    package_version = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))["version"]
    pyproject_version = tomllib.loads((ROOT / "python" / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    module_version = _module_dunder_version(ROOT / "python" / "src" / "cs2df" / "__init__.py")

    versions = {
        "package.json": package_version,
        "python/pyproject.toml": pyproject_version,
        "python/src/cs2df/__init__.py": module_version,
    }
    expected = package_version
    mismatches = {name: version for name, version in versions.items() if version != expected}
    if mismatches:
        print("Version mismatch:")
        for name, version in versions.items():
            marker = "!=" if name in mismatches else "=="
            print(f"  {name}: {version} {marker} {expected}")
        return 1

    print(f"All release versions match: {expected}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
