use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;
use zip::ZipArchive;

const EPS: f64 = 0.02;

const REQUIRED_KEYS: [&str; 11] = [
    "blinds",
    "bombs",
    "clutches",
    "damages",
    "grenades",
    "kills",
    "match",
    "playerEconomies",
    "playerStats",
    "players",
    "rounds",
];

const OPTIONAL_KEYS: [&str; 3] = ["duels", "replay", "shots"];

const REPLAY_COLS: [&str; 13] = [
    "x",
    "y",
    "z",
    "yaw",
    "pitch",
    "hp",
    "armor",
    "money",
    "equipValue",
    "weapon",
    "place",
    "flash",
    "flags",
];
const REPLAY_OPTIONAL_FRAME_COLS: [&str; 1] = ["grenades"];
const DUEL_COLS: [&str; 7] = ["x", "y", "z", "yaw", "pitch", "hp", "flash"];
const SHOT_COLS: [&str; 10] = [
    "tick", "weapon", "x", "y", "z", "vx", "vy", "vz", "yaw", "pitch",
];

fn embedded_schema_str(key: &str) -> Option<&'static str> {
    match key {
        "manifest" => Some(include_str!("../../../spec/manifest.schema.json")),
        "match" => Some(include_str!("../../../spec/match.schema.json")),
        "players" => Some(include_str!("../../../spec/players.schema.json")),
        "rounds" => Some(include_str!("../../../spec/rounds.schema.json")),
        "playerStats" => Some(include_str!("../../../spec/playerStats.schema.json")),
        "playerEconomies" => Some(include_str!("../../../spec/playerEconomies.schema.json")),
        "kills" => Some(include_str!("../../../spec/kills.schema.json")),
        "damages" => Some(include_str!("../../../spec/damages.schema.json")),
        "blinds" => Some(include_str!("../../../spec/blinds.schema.json")),
        "bombs" => Some(include_str!("../../../spec/bombs.schema.json")),
        "grenades" => Some(include_str!("../../../spec/grenades.schema.json")),
        "clutches" => Some(include_str!("../../../spec/clutches.schema.json")),
        "shots" => Some(include_str!("../../../spec/shots.schema.json")),
        "replay" => Some(include_str!("../../../spec/replay.schema.json")),
        "duels" => Some(include_str!("../../../spec/duels.schema.json")),
        _ => None,
    }
}

pub fn load_schemas(spec_dir: Option<&Path>) -> Result<HashMap<String, Value>> {
    let mut schemas = HashMap::new();
    if let Some(dir) = spec_dir {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if let Some(key) = name.strip_suffix(".schema.json") {
                    let text = std::fs::read_to_string(&path)?;
                    let val: Value = serde_json::from_str(&text)?;
                    schemas.insert(key.to_string(), val);
                }
            }
        }
    } else {
        for key in ["manifest"]
            .into_iter()
            .chain(REQUIRED_KEYS)
            .chain(OPTIONAL_KEYS)
        {
            if let Some(text) = embedded_schema_str(key) {
                let val: Value = serde_json::from_str(text)?;
                schemas.insert(key.to_string(), val);
            }
        }
    }
    Ok(schemas)
}

pub fn decode_delta(values: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(values.len());
    let mut acc = 0i64;
    for &v in values {
        acc += v;
        out.push(acc);
    }
    out
}

pub fn find_invalid_json_tokens(text: &str) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut in_str = false;
    let mut escape = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_str = true;
            i += 1;
            continue;
        }
        for tok in ["NaN", "-Infinity", "Infinity"] {
            if text[i..].starts_with(tok) {
                let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
                let after_idx = i + tok.len();
                let after_ok = after_idx >= bytes.len() || !is_ident_char(bytes[after_idx]);
                if before_ok && after_ok {
                    out.push((i, tok));
                    i += tok.len();
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub fn validate_zip(zip_path: &Path, spec_dir: Option<&Path>, strict: bool) -> Result<bool> {
    let name = zip_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| zip_path.display().to_string());
    println!(
        "Validating: {name}
"
    );

    let schemas = load_schemas(spec_dir)?;
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut data_by_key: HashMap<String, Value> = HashMap::new();

    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let zip_names: HashSet<String> = (0..archive.len())
        .filter_map(|i| {
            archive
                .by_index(i)
                .ok()
                .and_then(|f| f.name().ok().map(|s| s.into_owned()))
        })
        .collect();

    if !zip_names.contains("manifest.json") {
        push_err(&mut errors, "manifest.json: missing from ZIP");
        return Ok(finish(&errors, &warnings, strict));
    }

    let Some(manifest) = read_json_from_zip(&mut archive, "manifest.json", &mut errors) else {
        return Ok(finish(&errors, &warnings, strict));
    };
    validate_schema("manifest", &manifest, &schemas, &mut errors);

    let schema_version = manifest
        .get("schemaVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if schema_version != "cs2-demo-format/3.0" {
        push_err(
            &mut errors,
            &format!("manifest.json: unsupported schemaVersion '{schema_version}'"),
        );
    }

    let files_map = manifest.get("files").and_then(|v| v.as_object());
    for key in REQUIRED_KEYS {
        if !files_map.is_some_and(|m| m.contains_key(key)) {
            push_err(
                &mut errors,
                &format!("manifest.files: missing required key '{key}'"),
            );
        }
    }
    if let Some(m) = files_map {
        for key in m.keys() {
            if !REQUIRED_KEYS.contains(&key.as_str()) && !OPTIONAL_KEYS.contains(&key.as_str()) {
                push_err(&mut errors, &format!("manifest.files: unknown key '{key}'"));
            }
        }
        for (key, val) in m {
            let Some(filename) = val.as_str() else {
                continue;
            };
            if !zip_names.contains(filename) {
                if REQUIRED_KEYS.contains(&key.as_str()) {
                    push_err(
                        &mut errors,
                        &format!(
                            "{filename}: declared for required key '{key}' but missing from ZIP"
                        ),
                    );
                } else {
                    push_warn(
                        &mut warnings,
                        &format!("{filename}: declared optional key '{key}' but file is missing"),
                    );
                }
                continue;
            }
            let Some(value) = read_json_from_zip(&mut archive, filename, &mut errors) else {
                continue;
            };
            validate_schema(key, &value, &schemas, &mut errors);
            let count = value.as_array().map(|a| a.len()).unwrap_or(1);
            let unit = if count != 1 { "rows" } else { "row" };
            println!("  ✓ {filename} ({count} {unit})");
            data_by_key.insert(key.clone(), value);
        }
    }

    data_by_key.insert("manifest".to_string(), manifest);
    package_qa(&data_by_key, &mut errors);
    println!();
    Ok(finish(&errors, &warnings, strict))
}

pub fn validate_dir_fixture(dir: &Path, spec_dir: Option<&Path>, strict: bool) -> Result<bool> {
    let schemas = load_schemas(spec_dir)?;
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut data_by_key: HashMap<String, Value> = HashMap::new();

    let manifest_path = dir.join("manifest.json");
    if !manifest_path.exists() {
        push_err(&mut errors, "manifest.json: missing");
        return Ok(finish(&errors, &warnings, strict));
    }
    let raw_manifest = std::fs::read_to_string(&manifest_path)?;
    let manifest: Value = serde_json::from_str(&raw_manifest)?;
    validate_schema("manifest", &manifest, &schemas, &mut errors);

    if let Some(m) = manifest.get("files").and_then(|v| v.as_object()) {
        for (key, val) in m {
            let Some(filename) = val.as_str() else {
                continue;
            };
            let p = dir.join(filename);
            if !p.exists() {
                if REQUIRED_KEYS.contains(&key.as_str()) {
                    push_err(&mut errors, &format!("{filename}: missing"));
                } else {
                    push_warn(&mut warnings, &format!("{filename}: missing optional"));
                }
                continue;
            }
            let text = std::fs::read_to_string(&p)?;
            let invalid = find_invalid_json_tokens(&text);
            if !invalid.is_empty() {
                push_err(&mut errors, &format!("{filename}: invalid JSON tokens"));
                continue;
            }
            let value: Value = serde_json::from_str(&text)?;
            validate_schema(key, &value, &schemas, &mut errors);
            data_by_key.insert(key.clone(), value);
        }
    }

    data_by_key.insert("manifest".to_string(), manifest);
    package_qa(&data_by_key, &mut errors);
    Ok(finish(&errors, &warnings, strict))
}

fn push_err(errors: &mut Vec<String>, msg: &str) {
    errors.push(msg.to_string());
    println!("  ✗ {msg}");
}

fn push_warn(warnings: &mut Vec<String>, msg: &str) {
    warnings.push(msg.to_string());
    println!("  ⚠ {msg}");
}

fn read_json_from_zip(
    archive: &mut ZipArchive<File>,
    filename: &str,
    errors: &mut Vec<String>,
) -> Option<Value> {
    let mut entry = match archive.by_name(filename) {
        Ok(f) => f,
        Err(e) => {
            push_err(errors, &format!("{filename}: failed to open in ZIP — {e}"));
            return None;
        }
    };
    let mut raw = String::new();
    if let Err(e) = entry.read_to_string(&mut raw) {
        push_err(errors, &format!("{filename}: UTF-8 read error — {e}"));
        return None;
    }
    let invalid = find_invalid_json_tokens(&raw);
    if !invalid.is_empty() {
        let sample: Vec<String> = invalid
            .iter()
            .take(5)
            .map(|(off, tok)| format!("{tok}@{off}"))
            .collect();
        push_err(
            errors,
            &format!("{filename}: invalid JSON value(s): {}", sample.join(", ")),
        );
        return None;
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(v) => Some(v),
        Err(e) => {
            push_err(errors, &format!("{filename}: JSON parse error — {e}"));
            None
        }
    }
}

fn validate_schema(
    key: &str,
    value: &Value,
    schemas: &HashMap<String, Value>,
    errors: &mut Vec<String>,
) {
    let Some(schema) = schemas.get(key) else {
        push_err(errors, &format!("{key}: no JSON Schema found"));
        return;
    };
    let validator = match jsonschema::draft7::new(schema) {
        Ok(v) => v,
        Err(e) => {
            push_err(errors, &format!("{key}: invalid schema — {e}"));
            return;
        }
    };
    let schema_errors: Vec<_> = validator.iter_errors(value).collect();
    for e in schema_errors.iter().take(20) {
        let path_str = e.instance_path().to_string();
        let path = if path_str.is_empty() {
            "(root)"
        } else {
            &path_str
        };
        push_err(errors, &format!("{key}: [{path}] {e}"));
    }
    if schema_errors.len() > 20 {
        push_err(
            errors,
            &format!(
                "{key}: {} additional schema error(s)",
                schema_errors.len() - 20
            ),
        );
    }
}

fn as_slice(val: Option<&Value>) -> &[Value] {
    val.and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn package_qa(data: &HashMap<String, Value>, errors: &mut Vec<String>) {
    let players = as_slice(data.get("players"));
    let rounds = as_slice(data.get("rounds"));
    let stats = as_slice(data.get("playerStats"));
    let economies = as_slice(data.get("playerEconomies"));
    let kills = as_slice(data.get("kills"));
    let damages = as_slice(data.get("damages"));
    let blinds = as_slice(data.get("blinds"));
    let bombs = as_slice(data.get("bombs"));
    let grenades = as_slice(data.get("grenades"));
    let clutches = as_slice(data.get("clutches"));
    let shots = data.get("shots").and_then(|v| v.as_object());
    let replay = data.get("replay").and_then(|v| v.as_object());
    let duels = data.get("duels").and_then(|v| v.as_object());
    let match_obj = data.get("match").and_then(|v| v.as_object());

    let n_players = players.len();
    let team_by_index: HashMap<usize, &str> = players
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.get("teamKey").and_then(|v| v.as_str()).map(|t| (i, t)))
        .collect();

    let round_numbers: Vec<i64> = rounds
        .iter()
        .filter_map(|r| r.get("roundNumber").and_then(|v| v.as_i64()))
        .collect();
    let round_set: HashSet<i64> = round_numbers.iter().copied().collect();
    let rounds_by_number: HashMap<i64, &Value> = rounds
        .iter()
        .filter_map(|r| {
            r.get("roundNumber")
                .and_then(|v| v.as_i64())
                .map(|n| (n, r))
        })
        .collect();

    println!(
        "  • rows: players={n_players}, rounds={}, kills={}, damages={}",
        rounds.len(),
        kills.len(),
        damages.len()
    );

    if let Some(&max_r) = round_numbers.iter().max() {
        let mut sorted = round_numbers.clone();
        sorted.sort_unstable();
        let expected: Vec<i64> = (1..=max_r).collect();
        if sorted != expected {
            push_err(
                errors,
                "rounds.json: roundNumber must be continuous from 1 with no gaps/duplicates",
            );
        }
    }

    let mut bad_tick_rounds = Vec::new();
    for r in rounds {
        let rn = r.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(0);
        if r.get("teamASide") == r.get("teamBSide") {
            push_err(
                errors,
                &format!("rounds.json round {rn}: teamASide and teamBSide must differ"),
            );
        }
        let st = r.get("startTick").and_then(|v| v.as_i64()).unwrap_or(0);
        let fz = r.get("freezeEndTick").and_then(|v| v.as_i64()).unwrap_or(0);
        let et = r.get("endTick").and_then(|v| v.as_i64()).unwrap_or(0);
        if !(st < fz && fz <= et) {
            bad_tick_rounds.push(rn.to_string());
        }
    }
    if !bad_tick_rounds.is_empty() {
        let sample = bad_tick_rounds[..bad_tick_rounds.len().min(8)].join(", ");
        push_err(
            errors,
            &format!(
                "rounds.json: {} row(s) violate tick order start < freezeEnd <= end; sample rounds: {sample}",
                bad_tick_rounds.len()
            ),
        );
    }

    if let Some(m) = match_obj {
        let score_a = m
            .get("teamA")
            .and_then(|t| t.get("score"))
            .and_then(|v| v.as_i64());
        let score_b = m
            .get("teamB")
            .and_then(|t| t.get("score"))
            .and_then(|v| v.as_i64());
        let exp_a = rounds
            .iter()
            .filter(|r| r.get("winnerTeamKey").and_then(|v| v.as_str()) == Some("teamA"))
            .count() as i64;
        let exp_b = rounds
            .iter()
            .filter(|r| r.get("winnerTeamKey").and_then(|v| v.as_str()) == Some("teamB"))
            .count() as i64;
        if score_a != Some(exp_a) || score_b != Some(exp_b) {
            push_err(
                errors,
                &format!("match.json: score must equal round winners ({exp_a}:{exp_b})"),
            );
        }
    }

    for r in rounds {
        let winner = r.get("winnerTeamKey").and_then(|v| v.as_str());
        let expected = match winner {
            Some("teamA") => r.get("teamASide").and_then(|v| v.as_str()),
            Some("teamB") => r.get("teamBSide").and_then(|v| v.as_str()),
            _ => None,
        };
        if let Some(exp) = expected {
            if r.get("winnerSide").and_then(|v| v.as_str()) != Some(exp) {
                let rn = r.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(0);
                push_err(
                    errors,
                    &format!("rounds.json round {rn}: winnerSide must match winnerTeamKey side"),
                );
            }
        }
    }

    for (name, rows) in [
        ("kills", kills),
        ("damages", damages),
        ("blinds", blinds),
        ("bombs", bombs),
        ("grenades", grenades),
        ("clutches", clutches),
        ("player-economies", economies),
    ] {
        check_event_rounds(name, rows, &round_set, errors);
    }

    check_tick_windows("kills", kills, &rounds_by_number, &["tick"], errors);
    check_tick_windows("damages", damages, &rounds_by_number, &["tick"], errors);
    check_tick_windows("blinds", blinds, &rounds_by_number, &["tick"], errors);
    check_tick_windows("bombs", bombs, &rounds_by_number, &["tick"], errors);
    check_tick_windows(
        "grenades",
        grenades,
        &rounds_by_number,
        &["throwTick", "effectTick", "destroyTick"],
        errors,
    );
    check_tick_windows("clutches", clutches, &rounds_by_number, &["tick"], errors);

    for (name, rows, fields) in [
        (
            "kills",
            kills,
            [
                "killerIndex",
                "victimIndex",
                "assisterIndex",
                "flashAssisterIndex",
            ]
            .as_slice(),
        ),
        (
            "damages",
            damages,
            ["attackerIndex", "victimIndex"].as_slice(),
        ),
        (
            "blinds",
            blinds,
            ["flasherIndex", "flashedIndex"].as_slice(),
        ),
        ("bombs", bombs, ["actorIndex"].as_slice()),
        ("grenades", grenades, ["throwerIndex"].as_slice()),
        ("clutches", clutches, ["clutcherIndex"].as_slice()),
        ("player-economies", economies, ["playerIndex"].as_slice()),
        ("player-stats", stats, ["playerIndex"].as_slice()),
    ] {
        check_player_refs(name, rows, fields, n_players, errors);
    }

    let expected_economies = rounds.len() * n_players;
    let eco_keys: HashSet<(i64, i64)> = economies
        .iter()
        .filter_map(|r| {
            let rn = r.get("roundNumber")?.as_i64()?;
            let pi = r.get("playerIndex")?.as_i64()?;
            Some((rn, pi))
        })
        .collect();
    if eco_keys.len() != expected_economies {
        push_err(
            errors,
            &format!(
                "player-economies.json: expected {expected_economies} round/player rows, got {} unique rows",
                eco_keys.len()
            ),
        );
    }

    for d in damages {
        let raw = d.get("healthDamageRaw").and_then(|v| v.as_i64());
        let eff = d.get("healthDamage").and_then(|v| v.as_i64());
        let bef = d.get("victimHealthBefore").and_then(|v| v.as_i64());
        if let (Some(r), Some(e), Some(b)) = (raw, eff, bef) {
            if e != r.min(b) {
                let rn = d.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(0);
                let tk = d.get("tick").and_then(|v| v.as_i64()).unwrap_or(0);
                push_err(
                    errors,
                    &format!("damages.json round {rn} tick {tk}: healthDamage must equal min(healthDamageRaw, victimHealthBefore)"),
                );
            }
        }
    }

    let mut damage_by_player: HashMap<usize, i64> = HashMap::new();
    let mut utility_by_player: HashMap<usize, i64> = HashMap::new();
    for d in damages {
        let Some(atk) = d
            .get("attackerIndex")
            .and_then(|v| v.as_u64())
            .map(|x| x as usize)
        else {
            continue;
        };
        let Some(vic) = d
            .get("victimIndex")
            .and_then(|v| v.as_u64())
            .map(|x| x as usize)
        else {
            continue;
        };
        if atk == vic || team_by_index.get(&atk) == team_by_index.get(&vic) {
            continue;
        }
        let Some(health) = d.get("healthDamage").and_then(|v| v.as_i64()) else {
            continue;
        };
        *damage_by_player.entry(atk).or_insert(0) += health;
        if let Some(w) = d.get("weapon").and_then(|v| v.as_str()) {
            if matches!(w, "hegrenade" | "inferno" | "molotov" | "incendiary") {
                *utility_by_player.entry(atk).or_insert(0) += health;
            }
        }
    }

    for s in stats {
        let idx = s.get("playerIndex").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let rounds_count = s.get("rounds").and_then(|v| v.as_u64()).map(|x| x as usize);
        let label = format!("player-stats.json playerIndex={idx}");
        if rounds_count != Some(rounds.len()) {
            push_err(
                errors,
                &format!(
                    "{label}: rounds must equal rounds.length ({})",
                    rounds.len()
                ),
            );
        }
        let exp_dh = damage_by_player.get(&idx).copied().unwrap_or(0);
        if s.get("damageHealth").and_then(|v| v.as_i64()) != Some(exp_dh) {
            push_err(
                errors,
                &format!(
                    "{label}: damageHealth expected {exp_dh}, got {:?}",
                    s.get("damageHealth")
                ),
            );
        }
        let exp_ud = utility_by_player.get(&idx).copied().unwrap_or(0);
        if s.get("utilityDamage").and_then(|v| v.as_i64()) != Some(exp_ud) {
            push_err(
                errors,
                &format!(
                    "{label}: utilityDamage expected {exp_ud}, got {:?}",
                    s.get("utilityDamage")
                ),
            );
        }
        if let Some(rc) = rounds_count.filter(|&c| c > 0) {
            let dh = s
                .get("damageHealth")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let ud = s
                .get("utilityDamage")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let kr = s.get("kastRounds").and_then(|v| v.as_f64()).unwrap_or(0.0);
            expect_close(
                s.get("adr"),
                dh / rc as f64,
                errors,
                &format!("{label}: adr"),
            );
            expect_close(
                s.get("averageUtilityDamagePerRound"),
                ud / rc as f64,
                errors,
                &format!("{label}: averageUtilityDamagePerRound"),
            );
            expect_close(
                s.get("kast"),
                kr / rc as f64 * 100.0,
                errors,
                &format!("{label}: kast"),
            );
        }
        for field in [
            "firstKillCount",
            "firstDeathCount",
            "kastRounds",
            "oneKillCount",
            "twoKillCount",
            "threeKillCount",
            "fourKillCount",
            "fiveKillCount",
        ] {
            if let (Some(val), Some(rc)) = (
                s.get(field).and_then(|v| v.as_u64()).map(|x| x as usize),
                rounds_count,
            ) {
                if val > rc {
                    push_err(errors, &format!("{label}: {field} cannot exceed rounds"));
                }
            }
        }
        for n in ["One", "Two", "Three", "Four", "Five"] {
            let c = s.get(format!("vs{n}Count")).and_then(|v| v.as_i64());
            let w = s.get(format!("vs{n}WonCount")).and_then(|v| v.as_i64());
            let l = s.get(format!("vs{n}LostCount")).and_then(|v| v.as_i64());
            if let (Some(cv), Some(wv), Some(lv)) = (c, w, l) {
                if wv + lv != cv {
                    push_err(
                        errors,
                        &format!("{label}: vs{n}WonCount + vs{n}LostCount must equal vs{n}Count"),
                    );
                }
            }
        }
    }

    for k in kills {
        if k.get("flashAssist").and_then(|v| v.as_bool()) == Some(true)
            && k.get("flashAssisterIndex").is_none_or(|v| v.is_null())
        {
            let rn = k.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(0);
            let tk = k.get("tick").and_then(|v| v.as_i64()).unwrap_or(0);
            push_err(
                errors,
                &format!(
                    "kills.json round {rn} tick {tk}: flashAssist=true requires flashAssisterIndex"
                ),
            );
        }
    }

    for b in bombs {
        let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(bt, "planted" | "defused") && b.get("actorIndex").is_none_or(|v| v.is_null()) {
            let rn = b.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(0);
            let tk = b.get("tick").and_then(|v| v.as_i64()).unwrap_or(0);
            push_err(
                errors,
                &format!("bombs.json round {rn} tick {tk}: {bt} requires actorIndex"),
            );
        }
    }
    check_bomb_lifecycle(bombs, errors);

    for g in grenades {
        if let (Some(dt), Some(et)) = (
            g.get("destroyTick").and_then(|v| v.as_i64()),
            g.get("effectTick").and_then(|v| v.as_i64()),
        ) {
            if dt < et {
                let rn = g.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(0);
                let tt = g.get("throwTick").and_then(|v| v.as_i64()).unwrap_or(0);
                push_err(
                    errors,
                    &format!(
                        "grenades.json round {rn} tick {tt}: destroyTick must be >= effectTick"
                    ),
                );
            }
        }
    }

    if let Some(s) = shots {
        check_shots_stream(s, n_players, &round_set, &rounds_by_number, errors);
    }
    if let Some(r) = replay {
        check_replay_stream(r, n_players, &round_set, &rounds_by_number, errors);
    }
    if let Some(d) = duels {
        check_duels_stream(d, n_players, &round_set, &rounds_by_number, errors);
    }
}

fn check_shots_stream(
    shots: &serde_json::Map<String, Value>,
    n_players: usize,
    round_set: &HashSet<i64>,
    rounds_by_number: &HashMap<i64, &Value>,
    errors: &mut Vec<String>,
) {
    let wd_len = shots
        .get("weaponDict")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let tracks = shots
        .get("tracks")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for (ti, t) in tracks.iter().enumerate() {
        let label = format!("shots.json tracks[{ti}]");
        let rn = t.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(-1);
        if !round_set.contains(&rn) {
            push_err(
                errors,
                &format!("{label}: roundNumber {rn} not in rounds.json"),
            );
            continue;
        }
        if !index_ok(t.get("playerIndex"), n_players) {
            push_err(errors, &format!("{label}: playerIndex out of range"));
        }
        let lengths: HashSet<usize> = SHOT_COLS
            .iter()
            .map(|c| {
                t.get(c)
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0)
            })
            .collect();
        if lengths.len() > 1 {
            push_err(errors, &format!("{label}: column lengths differ"));
            continue;
        }
        if let Some(ws) = t.get("weapon").and_then(|v| v.as_array()) {
            for w in ws {
                let wi = w.as_i64().unwrap_or(-1);
                if wi < 0 || (wi as usize) >= wd_len {
                    push_err(
                        errors,
                        &format!("{label}: weapon index {wi} out of weaponDict range"),
                    );
                    break;
                }
            }
        }
        let raw_ticks: Vec<i64> = t
            .get("tick")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();
        let ticks = decode_delta(&raw_ticks);
        if let (Some(rd), Some(event_end)) = (
            rounds_by_number.get(&rn),
            round_event_end(rounds_by_number, rn),
        ) {
            let fz = rd
                .get("freezeEndTick")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if !ticks.is_empty() && !ticks.iter().all(|&tk| fz <= tk && tk <= event_end) {
                push_err(
                    errors,
                    &format!("{label}: decoded ticks fall outside the round window"),
                );
            }
        }
    }
}

fn check_track_frames(
    label: &str,
    track: &Value,
    cols: &[&str],
    frame_count: usize,
    n_players: usize,
    errors: &mut Vec<String>,
) {
    if !index_ok(track.get("playerIndex"), n_players) {
        push_err(errors, &format!("{label}: playerIndex out of range"));
    }
    let bad: Vec<String> = cols
        .iter()
        .filter_map(|c| {
            let len = track
                .get(c)
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            if len != frame_count {
                Some(format!("{c}={len}"))
            } else {
                None
            }
        })
        .collect();
    if !bad.is_empty() {
        push_err(
            errors,
            &format!(
                "{label}: column lengths != frameCount {frame_count}: {}",
                bad.join(", ")
            ),
        );
    }
}

fn check_replay_stream(
    replay: &serde_json::Map<String, Value>,
    n_players: usize,
    round_set: &HashSet<i64>,
    rounds_by_number: &HashMap<i64, &Value>,
    errors: &mut Vec<String>,
) {
    let wd_len = replay
        .get("weaponDict")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let pld_len = replay
        .get("placeDict")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let rounds = replay
        .get("rounds")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    for rd_obj in rounds {
        let rn = rd_obj
            .get("roundNumber")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        let label = format!("replay.json round {rn}");
        if !round_set.contains(&rn) {
            push_err(errors, &format!("{label}: roundNumber not in rounds.json"));
            continue;
        }
        let fc = rd_obj
            .get("frameCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let start = rd_obj
            .get("startTick")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let step = rd_obj.get("tickStep").and_then(|v| v.as_i64()).unwrap_or(1);
        if fc > 0 {
            if let (Some(rd), Some(event_end)) = (
                rounds_by_number.get(&rn),
                round_event_end(rounds_by_number, rn),
            ) {
                let last_tick = start + (fc as i64 - 1) * step;
                let fz = rd
                    .get("freezeEndTick")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if start < fz || last_tick > event_end {
                    push_err(
                        errors,
                        &format!("{label}: frame grid [{start}, {last_tick}] outside round window"),
                    );
                }
            }
        }
        if let Some(players) = rd_obj.get("players").and_then(|v| v.as_array()) {
            for (pi, track) in players.iter().enumerate() {
                let tlabel = format!("{label} players[{pi}]");
                check_track_frames(&tlabel, track, &REPLAY_COLS, fc, n_players, errors);
                for c in REPLAY_OPTIONAL_FRAME_COLS {
                    if let Some(arr) = track.get(c).and_then(|v| v.as_array()) {
                        if arr.len() != fc {
                            push_err(
                                errors,
                                &format!(
                                    "{tlabel}: optional column {c} length {} != frameCount {fc}",
                                    arr.len()
                                ),
                            );
                        }
                    }
                }
                if let Some(ws) = track.get("weapon").and_then(|v| v.as_array()) {
                    for w in ws {
                        let wi = w.as_i64().unwrap_or(-2);
                        if wi != -1 && (wi < 0 || (wi as usize) >= wd_len) {
                            push_err(
                                errors,
                                &format!("{tlabel}: weapon index {wi} out of weaponDict range"),
                            );
                            break;
                        }
                    }
                }
                if let Some(ps) = track.get("place").and_then(|v| v.as_array()) {
                    for p in ps {
                        let pi_val = p.as_i64().unwrap_or(-2);
                        if pi_val != -1 && (pi_val < 0 || (pi_val as usize) >= pld_len) {
                            push_err(
                                errors,
                                &format!("{tlabel}: place index {pi_val} out of placeDict range"),
                            );
                            break;
                        }
                    }
                }
            }
        }
        if let Some(projs) = rd_obj.get("projectiles").and_then(|v| v.as_array()) {
            for (qi, proj) in projs.iter().enumerate() {
                let nx = proj
                    .get("x")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let ny = proj
                    .get("y")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let nz = proj
                    .get("z")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if ny != nx || nz != nx {
                    push_err(
                        errors,
                        &format!("{label} projectiles[{qi}]: x/y/z lengths differ"),
                    );
                }
                if let Some(ti) = proj.get("throwerIndex") {
                    if !ti.is_null() && !index_ok(Some(ti), n_players) {
                        push_err(
                            errors,
                            &format!("{label} projectiles[{qi}]: throwerIndex out of range"),
                        );
                    }
                }
            }
        }
    }
}

fn check_duels_stream(
    duels: &serde_json::Map<String, Value>,
    n_players: usize,
    round_set: &HashSet<i64>,
    rounds_by_number: &HashMap<i64, &Value>,
    errors: &mut Vec<String>,
) {
    let windows = duels
        .get("windows")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for (wi, w) in windows.iter().enumerate() {
        let rn = w.get("roundNumber").and_then(|v| v.as_i64()).unwrap_or(-1);
        let label = format!("duels.json windows[{wi}] (round {rn})");
        if !round_set.contains(&rn) {
            push_err(errors, &format!("{label}: roundNumber not in rounds.json"));
            continue;
        }
        let fc = w.get("frameCount").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let start = w.get("startTick").and_then(|v| v.as_i64()).unwrap_or(0);
        let step = w.get("tickStep").and_then(|v| v.as_i64()).unwrap_or(1);
        if fc > 0 {
            if let (Some(rd), Some(event_end)) = (
                rounds_by_number.get(&rn),
                round_event_end(rounds_by_number, rn),
            ) {
                let last_tick = start + (fc as i64 - 1) * step;
                let fz = rd
                    .get("freezeEndTick")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if start < fz || last_tick > event_end {
                    push_err(
                        errors,
                        &format!("{label}: frame grid [{start}, {last_tick}] outside round window"),
                    );
                }
            }
        }
        let anchors = w
            .get("anchors")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if anchors.is_empty() {
            push_err(errors, &format!("{label}: empty anchors"));
        }
        let end_tick = start + (fc.saturating_sub(1) as i64) * step;
        for a in anchors {
            let tk = a.get("tick").and_then(|v| v.as_i64()).unwrap_or(0);
            if !(start <= tk && tk <= end_tick) {
                push_err(
                    errors,
                    &format!("{label}: anchor tick {tk} outside the window"),
                );
            }
            if !index_ok(a.get("victimIndex"), n_players) {
                push_err(errors, &format!("{label}: anchor victimIndex out of range"));
            }
            if let Some(ai) = a.get("attackerIndex") {
                if !ai.is_null() && !index_ok(Some(ai), n_players) {
                    push_err(
                        errors,
                        &format!("{label}: anchor attackerIndex out of range"),
                    );
                }
            }
        }
        if let Some(players) = w.get("players").and_then(|v| v.as_array()) {
            for (pi, track) in players.iter().enumerate() {
                check_track_frames(
                    &format!("{label} players[{pi}]"),
                    track,
                    &DUEL_COLS,
                    fc,
                    n_players,
                    errors,
                );
            }
        }
    }
}

fn index_ok(val: Option<&Value>, n_players: usize) -> bool {
    val.and_then(|v| v.as_i64())
        .is_some_and(|i| i >= 0 && (i as usize) < n_players)
}

fn check_player_refs(
    name: &str,
    rows: &[Value],
    fields: &[&str],
    n_players: usize,
    errors: &mut Vec<String>,
) {
    for row in rows {
        for &field in fields {
            if let Some(val) = row.get(field) {
                if !val.is_null() && !index_ok(Some(val), n_players) {
                    push_err(
                        errors,
                        &format!("{name}.json: {field} {val} is not a valid players.json index"),
                    );
                }
            }
        }
    }
}

fn check_event_rounds(
    name: &str,
    rows: &[Value],
    round_set: &HashSet<i64>,
    errors: &mut Vec<String>,
) {
    let mut missing: HashMap<i64, usize> = HashMap::new();
    for row in rows {
        let rn = row
            .get("roundNumber")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);
        if !round_set.contains(&rn) {
            *missing.entry(rn).or_insert(0) += 1;
        }
    }
    if !missing.is_empty() {
        let total: usize = missing.values().sum();
        let sample: Vec<String> = missing
            .iter()
            .take(8)
            .map(|(k, v)| format!("{k} ({v})"))
            .collect();
        push_err(
            errors,
            &format!(
                "{name}.json: {total} row(s) reference roundNumber not present in rounds.json; sample: {}",
                sample.join(", ")
            ),
        );
    }
}

fn round_event_end(rounds_by_number: &HashMap<i64, &Value>, round_number: i64) -> Option<i64> {
    let round_row = rounds_by_number.get(&round_number)?;
    if let Some(next_round) = rounds_by_number.get(&(round_number + 1)) {
        if let Some(st) = next_round.get("startTick").and_then(|v| v.as_i64()) {
            return Some(st - 1);
        }
    }
    round_row.get("endTick").and_then(|v| v.as_i64())
}

fn check_tick_windows(
    name: &str,
    rows: &[Value],
    rounds_by_number: &HashMap<i64, &Value>,
    fields: &[&str],
    errors: &mut Vec<String>,
) {
    let mut bad = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let Some(rn) = row.get("roundNumber").and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(round_row) = rounds_by_number.get(&rn) else {
            continue;
        };
        let Some(start) = round_row.get("freezeEndTick").and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(end) = round_event_end(rounds_by_number, rn) else {
            continue;
        };
        for &field in fields {
            let Some(tick) = row.get(field).and_then(|v| v.as_i64()) else {
                continue;
            };
            if tick < start || tick > end {
                bad.push(format!("row {index} round {rn} {field}={tick}"));
                break;
            }
        }
    }
    if !bad.is_empty() {
        let sample = bad[..bad.len().min(8)].join("; ");
        push_err(
            errors,
            &format!(
                "{name}.json: {} row(s) have ticks outside their round window; sample: {sample}",
                bad.len()
            ),
        );
    }
}

fn check_bomb_lifecycle(bombs: &[Value], errors: &mut Vec<String>) {
    let mut by_round: HashMap<i64, Vec<&Value>> = HashMap::new();
    for b in bombs {
        if let Some(rn) = b.get("roundNumber").and_then(|v| v.as_i64()) {
            by_round.entry(rn).or_default().push(b);
        }
    }
    let mut bad = Vec::new();
    let round_keys: BTreeSet<i64> = by_round.keys().copied().collect();
    for rn in round_keys {
        let mut rows = by_round.remove(&rn).unwrap_or_default();
        rows.sort_by_key(|b| b.get("tick").and_then(|v| v.as_i64()).unwrap_or(-1));
        let mut planted_tick: Option<i64> = None;
        for row in rows {
            let ev_type = row.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let tick = row.get("tick").and_then(|v| v.as_i64());
            if ev_type == "planted" {
                if let Some(t) = tick {
                    planted_tick = Some(t);
                }
            }
            if matches!(ev_type, "exploded" | "defused") {
                let ok = match (planted_tick, tick) {
                    (Some(pt), Some(t)) => t >= pt,
                    _ => false,
                };
                if !ok {
                    bad.push(format!("round {rn} {ev_type}@{}", tick.unwrap_or(-1)));
                    break;
                }
            }
        }
    }
    if !bad.is_empty() {
        let sample = bad[..bad.len().min(8)].join("; ");
        push_err(
            errors,
            &format!(
                "bombs.json: {} round(s) have terminal bomb events before planted; sample: {sample}",
                bad.len()
            ),
        );
    }
}

fn expect_close(actual: Option<&Value>, expected: f64, errors: &mut Vec<String>, label: &str) {
    let ok = actual
        .and_then(|v| v.as_f64())
        .is_some_and(|a| a.is_finite() && (a - expected).abs() <= EPS);
    if !ok {
        push_err(
            errors,
            &format!("{label} expected {expected:.3}, got {:?}", actual),
        );
    }
}

fn finish(errors: &[String], warnings: &[String], strict: bool) -> bool {
    let effective = errors.len() + if strict { warnings.len() } else { 0 };
    if effective > 0 {
        let mut parts = Vec::new();
        if !errors.is_empty() {
            parts.push(format!("{} error(s)", errors.len()));
        }
        if !warnings.is_empty() {
            let s = if strict { " [strict]" } else { "" };
            parts.push(format!("{} warning(s){s}", warnings.len()));
        }
        println!("❌ FAIL — {}", parts.join(", "));
        false
    } else if !warnings.is_empty() {
        println!(
            "⚠️  PASS with {} warning(s) (run --strict to treat as errors)",
            warnings.len()
        );
        true
    } else {
        println!("✅ PASS");
        true
    }
}
