#!/usr/bin/env python3
"""
Validate a cs2-demo-format v3 ZIP against the strict schema and package-level QA.

Thin wrapper around the cs2df reference CLI (python/src/cs2df/validate.py),
kept for backwards-compatible invocation:

    python tools/validate.py export.zip
    python tools/validate.py export.zip --spec path/to/spec/
    python tools/validate.py export.zip --strict

Equivalent: `cs2df validate export.zip` (pip/uv install ./python).
"""

import sys
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_REPO_ROOT / "python" / "src"))

from cs2df.cli import main  # noqa: E402

if __name__ == "__main__":
    sys.exit(main(["validate", *sys.argv[1:]]))
