//! Light level, palette, and the two independent title ladders.
//!
//! Invariants enforced and tested here:
//!
//! 1. The **core** (光核) depends only on *verified* (E3/E4) family count.
//! 2. **Self-attested (E2)** families never deepen the core and never raise the
//!    knowledge title. They are exposed separately as `attested_families`.
//! 3. **Received light** (来光) only affects the outer ring, never the core.
//! 4. Sharing never reduces anything: no function here takes away from the
//!    sender.
//!
//! Colour tokens come from product spec §4.3. Brightness and depth are computed
//! separately: a deep core keeps a warm, recognisable halo.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Light level, product spec §4.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LightLevel {
    /// 沉睡 — no trusted usage yet.
    Dormant,
    /// 微明 — 1 family.
    Faint,
    /// 生长 — 2–3 families.
    Growing,
    /// 炽热 — 4–6 families.
    Blazing,
    /// 深红 — 7+ families.
    Deep,
}

/// 知光称号 (knowledge title), product spec §6.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeTitle {
    /// 寻光者
    Seeker,
    /// 初明者
    FirstLight,
    /// 启迪者
    Enlightener,
    /// 探索者
    Explorer,
    /// 拓界者
    Frontier,
}

impl KnowledgeTitle {
    pub fn display_zh(self) -> &'static str {
        match self {
            Self::Seeker => "寻光者",
            Self::FirstLight => "初明者",
            Self::Enlightener => "启迪者",
            Self::Explorer => "探索者",
            Self::Frontier => "拓界者",
        }
    }

    /// Stable machine id used on the wire.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Seeker => "seeker",
            Self::FirstLight => "first_light",
            Self::Enlightener => "enlightener",
            Self::Explorer => "explorer",
            Self::Frontier => "frontier",
        }
    }
}

/// 传光称号 (transmission title), product spec §6.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransmissionTitle {
    /// 持光者
    Holder,
    /// 启蒙者
    Kindler,
    /// 引光者
    Guide,
    /// 播光者
    Sower,
    /// 传灯者
    LampBearer,
    /// 燎原者
    Wildfire,
}

impl TransmissionTitle {
    pub fn display_zh(self) -> &'static str {
        match self {
            Self::Holder => "持光者",
            Self::Kindler => "启蒙者",
            Self::Guide => "引光者",
            Self::Sower => "播光者",
            Self::LampBearer => "传灯者",
            Self::Wildfire => "燎原者",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Holder => "holder",
            Self::Kindler => "kindler",
            Self::Guide => "guide",
            Self::Sower => "sower",
            Self::LampBearer => "lamp_bearer",
            Self::Wildfire => "wildfire",
        }
    }
}

// ---------------------------------------------------------------------------
// Threshold tables. These are the single source of truth for both ladders.
// A tier applies when `value >= threshold`; the highest matching threshold wins.
// ---------------------------------------------------------------------------

/// (minimum verified families, title)
pub const KNOWLEDGE_LADDER: &[(u32, KnowledgeTitle)] = &[
    (0, KnowledgeTitle::Seeker),
    (1, KnowledgeTitle::FirstLight),
    (2, KnowledgeTitle::Enlightener),
    (4, KnowledgeTitle::Explorer),
    (7, KnowledgeTitle::Frontier),
];

/// (minimum activated nodes, title)
pub const TRANSMISSION_LADDER: &[(u32, TransmissionTitle)] = &[
    (0, TransmissionTitle::Holder),
    (1, TransmissionTitle::Kindler),
    (3, TransmissionTitle::Guide),
    (10, TransmissionTitle::Sower),
    (30, TransmissionTitle::LampBearer),
    (100, TransmissionTitle::Wildfire),
];

/// Map a verified family count to a light level.
pub fn light_level(verified_families: u32) -> LightLevel {
    match verified_families {
        0 => LightLevel::Dormant,
        1 => LightLevel::Faint,
        2..=3 => LightLevel::Growing,
        4..=6 => LightLevel::Blazing,
        _ => LightLevel::Deep,
    }
}

/// Map a verified family count to the knowledge title.
pub fn knowledge_title(verified_families: u32) -> KnowledgeTitle {
    highest(KNOWLEDGE_LADDER, verified_families)
}

/// Map an activated node count to the transmission title.
pub fn transmission_title(activated_nodes: u32) -> TransmissionTitle {
    highest(TRANSMISSION_LADDER, activated_nodes)
}

fn highest<T: Copy>(ladder: &[(u32, T)], value: u32) -> T {
    let mut chosen = ladder[0].1;
    for (threshold, title) in ladder {
        if value >= *threshold {
            chosen = *title;
        } else {
            break;
        }
    }
    chosen
}

/// Visual tokens for one light level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Palette {
    pub core_hex: &'static str,
    pub glow_hex: &'static str,
    pub glow_radius_px: u32,
    /// Ring opacity is kept independent of core depth so that a deep core still
    /// reads as "glowing" rather than "dark".
    pub glow_alpha_pct: u8,
    /// Idle breathing period; 0 means static.
    pub breath_period_ms: u32,
}

/// Canonical visual tokens, shared with the UI and every other platform.
///
/// The palette table below is bound to this file by the
/// `palette_matches_visual_tokens` test: editing one without the other fails CI.
pub const VISUAL_TOKENS_JSON: &str = include_str!("../../../packages/visual-system/tokens.json");

/// F3.6 当代水墨微光 palette (visual spec §3.1/§3.3).
///
/// These are *appearance* constants only. The level thresholds that decide which
/// one applies are business rules and are unchanged from F1.
pub const NIGHT_HEX: &str = "#0A0F16";
pub const MIST_BLUE_HEX: &str = "#77747A";
pub const IVORY_HEX: &str = "#B9B6B3";
/// Approved in F3.6-R3: the original #F1A99D read as candy on a real screen.
pub const CORAL_HEX: &str = "#D68B83";
pub const CRIMSON_HEX: &str = "#BC4054";
pub const DEEP_RED_HEX: &str = "#7D2038";
/// 2–3 family tier.
pub const GROWING_HEX: &str = "#DF7970";
/// Halos are always warmer and brighter than their core, so a deeper tier never
/// reads as less light (visual spec §3.3).
pub const HALO_DORMANT_HEX: &str = "#8A7D83";
pub const HALO_FAINT_HEX: &str = "#F2B3A7";
pub const HALO_GROWING_HEX: &str = "#E38A80";
pub const HALO_BLAZING_HEX: &str = "#DE7376";
pub const HALO_DEEP_HEX: &str = "#E28980";

pub fn palette(level: LightLevel) -> Palette {
    match level {
        LightLevel::Dormant => Palette {
            core_hex: MIST_BLUE_HEX,
            glow_hex: HALO_DORMANT_HEX,
            glow_radius_px: 20,
            glow_alpha_pct: 12,
            breath_period_ms: 12_000,
        },
        LightLevel::Faint => Palette {
            core_hex: CORAL_HEX,
            glow_hex: HALO_FAINT_HEX,
            glow_radius_px: 26,
            glow_alpha_pct: 20,
            breath_period_ms: 11_000,
        },
        LightLevel::Growing => Palette {
            core_hex: GROWING_HEX,
            glow_hex: HALO_GROWING_HEX,
            glow_radius_px: 32,
            glow_alpha_pct: 26,
            breath_period_ms: 10_000,
        },
        LightLevel::Blazing => Palette {
            core_hex: CRIMSON_HEX,
            glow_hex: HALO_BLAZING_HEX,
            glow_radius_px: 38,
            glow_alpha_pct: 30,
            breath_period_ms: 9_500,
        },
        // The deepest core is darkest, but its halo is the *brightest*: a higher
        // tier must never read as weaker light (visual spec §4.3 / §11).
        LightLevel::Deep => Palette {
            core_hex: DEEP_RED_HEX,
            glow_hex: HALO_DEEP_HEX,
            glow_radius_px: 42,
            glow_alpha_pct: 34,
            breath_period_ms: 9_500,
        },
    }
}

/// The full personal light state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LightState {
    /// E3/E4 families. The only input to `level` and `knowledge_title`.
    pub verified_families: u32,
    /// E2 families. Displayed separately, never counted.
    pub attested_families: u32,
    /// Light received from others. Affects the ring only.
    pub received_glow: u32,
    /// Activated nodes this identity helped. Input to `transmission_title`.
    pub activated_nodes: u32,
    pub level: LightLevel,
    pub knowledge_title: KnowledgeTitle,
    pub transmission_title: TransmissionTitle,
    pub palette: Palette,
}

/// Compute the personal light state.
///
/// `attested_families` and `received_glow` are accepted so the UI can show them,
/// but they are structurally excluded from every derived value.
pub fn compute_light_state(
    verified_families: u32,
    attested_families: u32,
    received_glow: u32,
    activated_nodes: u32,
) -> LightState {
    let level = light_level(verified_families);
    LightState {
        verified_families,
        attested_families,
        received_glow,
        activated_nodes,
        level,
        knowledge_title: knowledge_title(verified_families),
        transmission_title: transmission_title(activated_nodes),
        palette: palette(level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn tokens() -> Value {
        serde_json::from_str(VISUAL_TOKENS_JSON).expect("visual tokens must be valid json")
    }

    fn resolve_color(tokens: &Value, name: &str) -> String {
        tokens["colors"][name]
            .as_str()
            .unwrap_or_else(|| panic!("unknown colour token `{name}`"))
            .to_string()
    }

    fn level_name(level: LightLevel) -> &'static str {
        match level {
            LightLevel::Dormant => "DORMANT",
            LightLevel::Faint => "FAINT",
            LightLevel::Growing => "GROWING",
            LightLevel::Blazing => "BLAZING",
            LightLevel::Deep => "DEEP",
        }
    }

    #[test]
    fn palette_matches_visual_tokens() {
        let t = tokens();
        for level in [
            LightLevel::Dormant,
            LightLevel::Faint,
            LightLevel::Growing,
            LightLevel::Blazing,
            LightLevel::Deep,
        ] {
            let name = level_name(level);
            let spec = &t["levels"][name];
            let p = palette(level);

            let core = resolve_color(&t, spec["coreColor"].as_str().unwrap());
            let glow = resolve_color(&t, spec["glowColor"].as_str().unwrap());
            assert_eq!(p.core_hex, core, "{name} core colour drifted from tokens.json");
            assert_eq!(p.glow_hex, glow, "{name} glow colour drifted from tokens.json");

            assert_eq!(
                p.glow_radius_px,
                spec["haloRadiusPx"].as_u64().unwrap() as u32,
                "{name} halo radius drifted"
            );
            let expected_alpha = (spec["haloAlpha"].as_f64().unwrap() * 100.0).round() as u8;
            assert_eq!(p.glow_alpha_pct, expected_alpha, "{name} halo alpha drifted");
            assert_eq!(
                p.breath_period_ms,
                spec["breathPeriodMs"].as_u64().unwrap() as u32,
                "{name} breath period drifted"
            );
        }
    }

    #[test]
    fn tokens_cover_every_colour_reference() {
        let t = tokens();
        for (name, spec) in t["levels"].as_object().expect("levels object") {
            for key in ["coreColor", "glowColor"] {
                let token = spec[key].as_str().unwrap_or_else(|| panic!("{name}.{key}"));
                assert!(
                    t["colors"].get(token).is_some(),
                    "{name}.{key} references missing colour `{token}`"
                );
            }
        }
    }

    #[test]
    fn reduce_motion_token_matches_the_rule() {
        let t = tokens();
        assert_eq!(t["motion"]["reduceMotion"]["staticFrame"], true);
        assert_eq!(t["motion"]["reduceMotion"]["disablesBreath"], true);
        assert_eq!(t["motion"]["reduceMotion"]["disablesFloat"], true);
    }

    #[test]
    fn far_pulse_rate_limit_matches_product_spec() {
        // Product spec §14.3: at most two distant pulses per minute.
        let t = tokens();
        assert_eq!(t["motion"]["farPulse"]["maxPerMinute"].as_u64(), Some(2));
    }

    #[test]
    fn knowledge_ladder_boundaries() {
        let cases = [
            (0u32, KnowledgeTitle::Seeker),
            (1, KnowledgeTitle::FirstLight),
            (2, KnowledgeTitle::Enlightener),
            (3, KnowledgeTitle::Enlightener),
            (4, KnowledgeTitle::Explorer),
            (6, KnowledgeTitle::Explorer),
            (7, KnowledgeTitle::Frontier),
            (99, KnowledgeTitle::Frontier),
        ];
        for (n, expected) in cases {
            assert_eq!(knowledge_title(n), expected, "count {n}");
        }
    }

    #[test]
    fn transmission_ladder_boundaries() {
        let cases = [
            (0u32, TransmissionTitle::Holder),
            (1, TransmissionTitle::Kindler),
            // 2 has no threshold of its own: it stays at the last tier reached.
            (2, TransmissionTitle::Kindler),
            (3, TransmissionTitle::Guide),
            (9, TransmissionTitle::Guide),
            (10, TransmissionTitle::Sower),
            (29, TransmissionTitle::Sower),
            (30, TransmissionTitle::LampBearer),
            (99, TransmissionTitle::LampBearer),
            (100, TransmissionTitle::Wildfire),
            (1000, TransmissionTitle::Wildfire),
        ];
        for (n, expected) in cases {
            assert_eq!(transmission_title(n), expected, "nodes {n}");
        }
    }

    #[test]
    fn light_level_boundaries() {
        assert_eq!(light_level(0), LightLevel::Dormant);
        assert_eq!(light_level(1), LightLevel::Faint);
        assert_eq!(light_level(2), LightLevel::Growing);
        assert_eq!(light_level(3), LightLevel::Growing);
        assert_eq!(light_level(4), LightLevel::Blazing);
        assert_eq!(light_level(6), LightLevel::Blazing);
        assert_eq!(light_level(7), LightLevel::Deep);
        assert_eq!(light_level(u32::MAX), LightLevel::Deep);
    }

    #[test]
    fn e2_never_deepens_the_core() {
        // This is the D-03 rule, enforced structurally.
        let base = compute_light_state(0, 0, 0, 0);
        let with_e2 = compute_light_state(0, 9, 0, 0);
        assert_eq!(base.level, LightLevel::Dormant);
        assert_eq!(with_e2.level, LightLevel::Dormant);
        assert_eq!(base.knowledge_title, with_e2.knowledge_title);
        assert_eq!(base.palette, with_e2.palette);
        // but E2 is still visible
        assert_eq!(with_e2.attested_families, 9);
    }

    #[test]
    fn received_light_only_affects_the_ring() {
        let without = compute_light_state(2, 0, 0, 0);
        let with = compute_light_state(2, 0, 50, 0);
        assert_eq!(without.level, with.level);
        assert_eq!(without.knowledge_title, with.knowledge_title);
        assert_eq!(without.palette, with.palette);
        assert_eq!(with.received_glow, 50);
    }

    #[test]
    fn sharing_never_reduces_the_sender() {
        // Transmission titles are driven by activated nodes; the helper's own
        // verified family count is not an input, so it cannot drop.
        let before = compute_light_state(3, 0, 0, 5);
        let after = compute_light_state(3, 0, 0, 6);
        assert_eq!(before.level, after.level);
        assert_eq!(before.knowledge_title, after.knowledge_title);
        assert_eq!(before.palette, after.palette);
        assert!(matches!(after.transmission_title, TransmissionTitle::Guide));
    }

    #[test]
    fn deep_core_keeps_a_warm_halo() {
        let deep = palette(LightLevel::Deep);
        assert_eq!(deep.core_hex, DEEP_RED_HEX);
        assert_ne!(deep.glow_hex, DEEP_RED_HEX, "halo must differ from the core");
        assert!(deep.glow_alpha_pct > 0, "deep core must still glow");
        // Brightness is not reduced as depth increases.
        let blazing = palette(LightLevel::Blazing);
        assert!(deep.glow_alpha_pct >= blazing.glow_alpha_pct);
        assert!(deep.glow_radius_px >= blazing.glow_radius_px);
    }

    #[test]
    fn palette_tokens_match_spec() {
        assert_eq!(palette(LightLevel::Faint).core_hex, CORAL_HEX);
        assert_eq!(palette(LightLevel::Blazing).core_hex, CRIMSON_HEX);
        assert_eq!(palette(LightLevel::Deep).core_hex, DEEP_RED_HEX);
        assert_eq!(palette(LightLevel::Dormant).core_hex, MIST_BLUE_HEX);
    }

    #[test]
    fn titles_have_stable_slugs_and_display_text() {
        assert_eq!(KnowledgeTitle::Seeker.display_zh(), "寻光者");
        assert_eq!(KnowledgeTitle::Frontier.display_zh(), "拓界者");
        assert_eq!(TransmissionTitle::Wildfire.display_zh(), "燎原者");
        assert_eq!(KnowledgeTitle::FirstLight.slug(), "first_light");
        assert_eq!(TransmissionTitle::LampBearer.slug(), "lamp_bearer");
    }
}
