# cs2df

`cs2df` is the reference Python exporter and validator for the
[`cs2-demo-format`](https://github.com/Starfie1d1272/cs2-demo-format) v3 ZIP
contract.

It parses CS2 `.dem` files with
[`demoparser2`](https://github.com/LaihoE/demoparser), writes strict v3 ZIP
packages, and validates exported packages with schema plus package-level QA.

## Setup

```bash
uv sync
```

The CLI entrypoint is `cs2df`.

## Export

```bash
# Standard profile: required files + shots.json + replay.json
uv run cs2df export match.dem

# Research profile: also emit full-tick duels.json windows
uv run cs2df export match.dem --research

# Choose an output path
uv run cs2df export match.dem -o match.zip
```

The default ZIP compression level is `3`, chosen from local benchmark results as
a speed/size balance. Use a higher level when smaller ZIPs matter more:

```bash
uv run cs2df export match.dem --compress-level 6
uv run cs2df export match.dem --compress-level 9
```

## Batch Export

```bash
uv run cs2df export-batch ./demos --workers 8 --descriptive
```

`export-batch` scans one directory non-recursively for `.dem` files and writes
one ZIP per demo. It also writes `report.json` next to the outputs with:

- per-demo success/failure status
- output ZIP size
- source demo size
- compression level
- total duration
- aggregate throughput
- parse/package/write stage timings

Bad demos are reported as failed rows; a single parser failure does not crash the
whole batch. Use `--fail-fast` when you want the batch to stop after the first
failed demo.

## Validate

```bash
uv run cs2df validate match.zip
uv run cs2df validate match.zip --strict
```

Validation checks JSON Schema and package-level invariants such as cross-file
player indexes, round windows, column lengths, weapon dictionary indexes, and
formal-round consistency.

## Role in the Repository

This is a reference implementation, not the contract itself. The authoritative
contract lives in [`../schemas/index.ts`](../schemas/index.ts) and
the generated JSON Schemas in the repository's `spec/` directory. Any producer
that emits a ZIP passing strict validation is conformant.

The exporter also serves as the performance baseline for the v3 format:
per-frame data stays in pandas DataFrames until the columnar stream builders
materialize compact integer arrays for JSON serialization.

Event-extraction logic was originally ported from `cs2-demo-analysis-kit`
(and before that `DrEAmSs59/CS2-insight-agent`, with the author's permission).
