# cs2df — cs2-demo-format reference exporter & validator

Reference producer CLI for the [cs2-demo-format](../README.md) v3 contract.
Parses a CS2 `.dem` with [demoparser2](https://github.com/LaihoE/demoparser)
and emits a strict v3 ZIP package.

```bash
uv sync                                  # set up the environment
uv run cs2df export match.dem            # → match.zip (standard profile)
uv run cs2df export match.dem --research # + duels.json (full-tick combat windows)
uv run cs2df validate match.zip          # schema + package-level QA
```

This is a *reference implementation*: the contract itself lives in
`schemas/index.ts` + `spec/*.schema.json`, and any producer that emits valid
packages is conformant. The code here doubles as the performance baseline —
columnar DataFrame→numpy delta encoding with no per-row dict materialization.

Event-extraction logic was originally ported from `cs2-demo-analysis-kit`
(and before that DrEAmSs59/CS2-insight-agent, with the author's permission).
