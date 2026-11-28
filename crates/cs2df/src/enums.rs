use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum TeamKey {
    #[serde(rename = "teamA")]
    TeamA,
    #[serde(rename = "teamB")]
    TeamB,
}

impl TeamKey {
    #[inline]
    pub fn opposite(self) -> Self {
        match self {
            Self::TeamA => Self::TeamB,
            Self::TeamB => Self::TeamA,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Side {
    #[serde(rename = "t")]
    T,
    #[serde(rename = "ct")]
    Ct,
}

impl Side {
    #[inline]
    pub fn opposite(self) -> Self {
        match self {
            Self::T => Self::Ct,
            Self::Ct => Self::T,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GrenadeType {
    Flashbang,
    Smoke,
    Molotov,
    Incendiary,
    Hegrenade,
    Decoy,
}

impl GrenadeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flashbang => "flashbang",
            Self::Smoke => "smoke",
            Self::Molotov => "molotov",
            Self::Incendiary => "incendiary",
            Self::Hegrenade => "hegrenade",
            Self::Decoy => "decoy",
        }
    }

    pub fn from_weapon(weapon: &str) -> Option<Self> {
        match weapon.trim().to_ascii_lowercase().as_str() {
            "smokegrenade" => Some(Self::Smoke),
            "flashbang" => Some(Self::Flashbang),
            "hegrenade" => Some(Self::Hegrenade),
            "molotov" => Some(Self::Molotov),
            "incgrenade" => Some(Self::Incendiary),
            "decoy" => Some(Self::Decoy),
            _ => None,
        }
    }

    pub fn from_projectile_class(class_name: &str) -> Option<Self> {
        match class_name.trim() {
            "CSmokeGrenadeProjectile" => Some(Self::Smoke),
            "CFlashbangProjectile" => Some(Self::Flashbang),
            "CHEGrenadeProjectile" => Some(Self::Hegrenade),
            "CMolotovProjectile" => Some(Self::Molotov),
            "CDecoyProjectile" => Some(Self::Decoy),
            _ => None,
        }
    }

    pub fn from_inventory_item(name: &str) -> Option<Self> {
        match name {
            "Smoke Grenade" => Some(Self::Smoke),
            "Flashbang" => Some(Self::Flashbang),
            "High Explosive Grenade" => Some(Self::Hegrenade),
            "Incendiary Grenade" => Some(Self::Incendiary),
            "Molotov" => Some(Self::Molotov),
            "Decoy Grenade" => Some(Self::Decoy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EconomyType {
    Pistol,
    Eco,
    Semi,
    Force,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BombEventKind {
    PlantBegin,
    Planted,
    DefuseBegin,
    Defused,
    Exploded,
    Dropped,
    PickedUp,
}

pub fn normalize_hitgroup(raw: Option<&str>) -> &'static str {
    let s = raw.unwrap_or("").trim().to_ascii_lowercase();
    match s.as_str() {
        "head" => "head",
        "chest" => "chest",
        "stomach" => "stomach",
        "leftarm" | "left arm" | "left_arm" => "left_arm",
        "rightarm" | "right arm" | "right_arm" => "right_arm",
        "leftleg" | "left leg" | "left_leg" => "left_leg",
        "rightleg" | "right leg" | "right_leg" => "right_leg",
        "gear" => "gear",
        "neck" => "neck",
        "generic" => "generic",
        _ => "generic",
    }
}

fn round_end_reason_from_code(code: i64) -> &'static str {
    match code {
        1 => "target_bombed",
        7 => "bomb_defused",
        8 => "ct_win",
        9 => "t_win",
        12 => "time_ran_out",
        _ => "time_ran_out",
    }
}

pub fn normalize_round_end_reason(raw_str: Option<&str>, raw_num: Option<i64>) -> &'static str {
    if let Some(code) = raw_num {
        return round_end_reason_from_code(code);
    }
    let text = match raw_str {
        Some(t) => t.trim(),
        None => return "time_ran_out",
    };
    if text.is_empty() {
        return "time_ran_out";
    }
    let key = text.to_ascii_lowercase().replace(' ', "_");
    match key.as_str() {
        "t_killed" | "t_eliminated" | "ct_win" => "ct_win",
        "ct_killed" | "ct_eliminated" | "t_win" => "t_win",
        "bomb_exploded" | "target_bombed" => "target_bombed",
        "bomb_defused" => "bomb_defused",
        "draw" | "round_draw" | "time_ran_out" => "time_ran_out",
        _ => {
            if let Ok(code) = text.parse::<i64>() {
                round_end_reason_from_code(code)
            } else {
                "time_ran_out"
            }
        }
    }
}

fn is_pistol_item(name: &str) -> bool {
    matches!(
        name,
        "Glock-18"
            | "USP-S"
            | "P2000"
            | "Dual Berettas"
            | "P250"
            | "Five-SeveN"
            | "Tec-9"
            | "CZ75-Auto"
            | "Desert Eagle"
            | "R8 Revolver"
    )
}

fn is_primary_item(name: &str) -> bool {
    matches!(
        name,
        "MAC-10"
            | "MP9"
            | "MP7"
            | "MP5-SD"
            | "UMP-45"
            | "P90"
            | "PP-Bizon"
            | "Galil AR"
            | "FAMAS"
            | "AK-47"
            | "M4A4"
            | "M4A1-S"
            | "SSG 08"
            | "SG 553"
            | "AUG"
            | "AWP"
            | "G3SG1"
            | "SCAR-20"
            | "Nova"
            | "XM1014"
            | "Sawed-Off"
            | "MAG-7"
            | "M249"
            | "Negev"
    )
}

pub fn classify_inventory(
    items: &[String],
) -> (Option<String>, Option<String>, usize, Vec<GrenadeType>) {
    let mut primary: Option<String> = None;
    let mut secondary: Option<String> = None;
    let mut grenade_types: Vec<GrenadeType> = Vec::new();
    for it in items {
        let name = it.trim();
        if let Some(gtype) = GrenadeType::from_inventory_item(name) {
            grenade_types.push(gtype);
        } else if primary.is_none() && is_primary_item(name) {
            primary = Some(name.to_string());
        } else if secondary.is_none() && is_pistol_item(name) {
            secondary = Some(name.to_string());
        }
    }
    let count = grenade_types.len();
    (primary, secondary, count, grenade_types)
}

fn weapon_display_to_canonical(lower: &str) -> Option<&'static str> {
    match lower {
        // Pistols
        "glock-18" => Some("glock"),
        "usp-s" => Some("usp_silencer"),
        "p2000" => Some("hkp2000"),
        "dual berettas" => Some("elite"),
        "p250" => Some("p250"),
        "five-seven" => Some("fiveseven"),
        "tec-9" => Some("tec9"),
        "cz75-auto" => Some("cz75a"),
        "desert eagle" => Some("deagle"),
        "r8 revolver" => Some("revolver"),
        // SMG
        "mac-10" => Some("mac10"),
        "mp9" => Some("mp9"),
        "mp7" => Some("mp7"),
        "mp5-sd" => Some("mp5sd"),
        "ump-45" => Some("ump45"),
        "p90" => Some("p90"),
        "pp-bizon" => Some("bizon"),
        // Rifle / sniper
        "galil ar" => Some("galilar"),
        "famas" => Some("famas"),
        "ak-47" => Some("ak47"),
        "m4a4" => Some("m4a1"),
        "m4a1-s" => Some("m4a1_silencer"),
        "ssg 08" => Some("ssg08"),
        "sg 553" => Some("sg556"),
        "aug" => Some("aug"),
        "awp" => Some("awp"),
        "g3sg1" => Some("g3sg1"),
        "scar-20" => Some("scar20"),
        // Shotgun / LMG
        "nova" => Some("nova"),
        "xm1014" => Some("xm1014"),
        "sawed-off" => Some("sawedoff"),
        "mag-7" => Some("mag7"),
        "m249" => Some("m249"),
        "negev" => Some("negev"),
        // Utility / equipment
        "smoke grenade" => Some("smokegrenade"),
        "flashbang" => Some("flashbang"),
        "high explosive grenade" => Some("hegrenade"),
        "incendiary grenade" => Some("incgrenade"),
        "molotov" => Some("molotov"),
        "decoy grenade" => Some("decoy"),
        "zeus x27" => Some("taser"),
        "c4 explosive" | "c4" => Some("c4"),
        "knife" => Some("knife"),
        "knife_t" => Some("knife_t"),
        _ => None,
    }
}

pub fn normalize_weapon_name(raw: Option<&str>) -> Option<String> {
    let text = raw?.trim();
    if text.is_empty()
        || text.chars().all(|c| c.is_ascii_digit())
        || matches!(text.to_ascii_lowercase().as_str(), "nan" | "none" | "null")
    {
        return None;
    }
    let mut lower = text.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_prefix("weapon_") {
        lower = stripped.to_string();
    }
    if let Some(canon) = weapon_display_to_canonical(&lower) {
        return Some(canon.to_string());
    }
    let ident: String = lower
        .chars()
        .map(|c| if c == '-' || c == ' ' { '_' } else { c })
        .collect();
    let mut chars = ident.chars();
    let first_alpha = chars.next().is_some_and(|c| c.is_alphabetic());
    if first_alpha && chars.all(|c| c.is_alphanumeric() || c == '_') {
        Some(ident)
    } else {
        None
    }
}

pub fn bomb_site_from_place(place: Option<&str>) -> Option<&'static str> {
    let p = place.unwrap_or("").trim().to_ascii_lowercase();
    match p.as_str() {
        "bombsitea" => Some("a"),
        "bombsiteb" => Some("b"),
        _ => None,
    }
}
