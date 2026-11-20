//! Model registry: the model-family dictionary and its normalization rules.
//!
//! The registry is **data**, not code (see `data/model-registry.v1.json`). It is
//! shared with every other platform so that a family decision cannot drift
//! between clients.
//!
//! The single most important rule here: **a family is never inferred from a
//! string that is not explicitly listed.** F0 proved on real data that naive
//! counting turns 1 real family into 9 "families" and awards the top title.

use crate::CoreError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Canonical registry shipped with this build. Servers may publish newer
/// versions, but an unverified registry must never replace this one.
pub const BUNDLED_REGISTRY_JSON: &str = include_str!("../../../data/model-registry.v1.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Exact,
    Prefix,
}

/// What a raw model string actually is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Disposition {
    /// A genuine model family; countable.
    Family { family: String },
    /// A label that is not a model at all (internal reviewer, subagent, task).
    TaskLabel { reason: String },
    /// Nothing matched. Never countable, never guessed.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Family {
    pub slug: String,
    pub provider: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientPrefix {
    pub prefix: String,
    pub client: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Alias {
    pub raw: String,
    pub match_kind: MatchKind,
    pub disposition: Disposition,
}

/// Outcome of normalizing one raw model string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Normalization {
    /// The string exactly as the client recorded it.
    pub raw: String,
    /// Family slug when the disposition is `Family`.
    pub family: Option<String>,
    /// Client that embedded its own name in the model string, if any.
    pub client: Option<String>,
    pub disposition: Disposition,
    /// Which alias rule fired; `None` when nothing matched.
    pub matched_rule: Option<String>,
    /// Registry version used for this decision, recorded on every event.
    pub registry_semver: String,
}

impl Disposition {
    pub fn is_family(&self) -> bool {
        matches!(self, Self::Family { .. })
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

impl Normalization {
    pub fn is_family(&self) -> bool {
        matches!(self.disposition, Disposition::Family { .. })
    }

    pub fn is_task_label(&self) -> bool {
        matches!(self.disposition, Disposition::TaskLabel { .. })
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self.disposition, Disposition::Unknown)
    }
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    semver: String,
    families: Vec<Family>,
    family_index: BTreeMap<String, Family>,
    client_prefixes: Vec<ClientPrefix>,
    exact: BTreeMap<String, Disposition>,
    /// Prefix rules, longest pattern first (deterministic, no re-sorting).
    prefixes: Vec<(String, Disposition)>,
}

#[derive(Deserialize)]
struct RawRegistry {
    semver: String,
    families: Vec<Family>,
    #[serde(default)]
    client_prefixes: Vec<RawClientPrefix>,
    aliases: Vec<RawAlias>,
}

#[derive(Deserialize)]
struct RawClientPrefix {
    prefix: String,
    client: String,
}

#[derive(Deserialize)]
struct RawAlias {
    raw: String,
    #[serde(rename = "match")]
    match_kind: MatchKind,
    disposition: Disposition,
}

impl ModelRegistry {
    /// Parse and validate a registry document.
    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        let raw: RawRegistry =
            serde_json::from_str(json).map_err(|e| CoreError::Registry(format!("parse: {e}")))?;
        if raw.semver.trim().is_empty() {
            return Err(CoreError::Registry("empty semver".into()));
        }

        let mut family_index = BTreeMap::new();
        for f in &raw.families {
            if f.slug.trim().is_empty() {
                return Err(CoreError::Registry("family with empty slug".into()));
            }
            if family_index.insert(f.slug.clone(), f.clone()).is_some() {
                return Err(CoreError::Registry(format!("duplicate family slug: {}", f.slug)));
            }
        }

        let mut exact: BTreeMap<String, Disposition> = BTreeMap::new();
        let mut prefixes: Vec<(String, Disposition)> = Vec::new();

        for alias in &raw.aliases {
            let key = alias.raw.trim().to_lowercase();
            if key.is_empty() {
                return Err(CoreError::Registry("alias with empty raw".into()));
            }
            if alias.disposition.is_unknown() {
                return Err(CoreError::Registry(format!(
                    "alias `{key}` must not declare an 'unknown' disposition"
                )));
            }
            if let Disposition::Family { family } = &alias.disposition {
                if !family_index.contains_key(family) {
                    return Err(CoreError::Registry(format!(
                        "alias `{key}` points at unknown family `{family}`"
                    )));
                }
            }
            match alias.match_kind {
                MatchKind::Exact => {
                    if exact.insert(key.clone(), alias.disposition.clone()).is_some() {
                        return Err(CoreError::Registry(format!("duplicate exact alias: {key}")));
                    }
                }
                MatchKind::Prefix => prefixes.push((key, alias.disposition.clone())),
            }
        }

        // Longest pattern first so `deepseek-r1` would beat `deepseek-`.
        prefixes.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

        let mut client_prefixes: Vec<ClientPrefix> = raw
            .client_prefixes
            .into_iter()
            .map(|c| ClientPrefix { prefix: c.prefix.to_lowercase(), client: c.client })
            .collect();
        client_prefixes.sort_by(|a, b| b.prefix.len().cmp(&a.prefix.len()));

        Ok(Self {
            semver: raw.semver,
            families: raw.families,
            family_index,
            client_prefixes,
            exact,
            prefixes,
        })
    }

    /// The registry compiled into this build.
    pub fn bundled() -> Self {
        Self::from_json(BUNDLED_REGISTRY_JSON).expect("bundled registry must be valid")
    }

    pub fn semver(&self) -> &str {
        &self.semver
    }

    pub fn families(&self) -> &[Family] {
        &self.families
    }

    pub fn family(&self, slug: &str) -> Option<&Family> {
        self.family_index.get(slug)
    }

    /// Normalize one raw model string. Never guesses.
    pub fn normalize(&self, raw_model: &str) -> Normalization {
        let raw = raw_model.trim().to_string();
        if raw.is_empty() {
            return self.result(raw, Disposition::Unknown, None, None);
        }

        let lowered = raw.to_lowercase();

        // 1. A client may embed its own name: `chatgpt-web/gpt-6-sol`.
        for cp in &self.client_prefixes {
            let Some(rest) = lowered.strip_prefix(cp.prefix.as_str()) else {
                continue;
            };
            let rest = rest.trim();
            if rest.is_empty() {
                break;
            }
            if let Some((disposition, rule)) = self.lookup(rest) {
                return self.result(
                    raw,
                    disposition,
                    Some(format!("{}{}", cp.prefix, rule)),
                    Some(cp.client.clone()),
                );
            }
            break;
        }

        // 2. Direct match on the whole string.
        match self.lookup(&lowered) {
            Some((disposition, rule)) => self.result(raw, disposition, Some(rule), None),
            None => self.result(raw, Disposition::Unknown, None, None),
        }
    }

    fn lookup(&self, lowered: &str) -> Option<(Disposition, String)> {
        if let Some(d) = self.exact.get(lowered) {
            return Some((d.clone(), lowered.to_string()));
        }
        for (pattern, d) in &self.prefixes {
            if lowered.starts_with(pattern.as_str()) {
                return Some((d.clone(), format!("prefix:{pattern}")));
            }
        }
        None
    }

    fn result(
        &self,
        raw: String,
        disposition: Disposition,
        matched_rule: Option<String>,
        client: Option<String>,
    ) -> Normalization {
        let family = match &disposition {
            Disposition::Family { family } => Some(family.clone()),
            _ => None,
        };
        Normalization {
            raw,
            family,
            client,
            disposition,
            matched_rule,
            registry_semver: self.semver.clone(),
        }
    }

    /// All family slugs, for diagnostics and UI.
    pub fn family_slugs(&self) -> BTreeSet<&str> {
        self.family_index.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::bundled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Expect {
        Family(&'static str),
        TaskLabel,
        Unknown,
    }

    /// The exact 9 distinct values observed on a real machine during F0.
    const F0_SAMPLES: &[(&str, Expect)] = &[
        ("gpt-6-sol", Expect::Family("gpt")),
        ("gpt-5.6-sol", Expect::Family("gpt")),
        ("gpt-5.6-terra", Expect::Family("gpt")),
        ("gpt-6.1-sol", Expect::Family("gpt")),
        ("gpt-6-astra", Expect::Family("gpt")),
        ("chatgpt-web/gpt-6-sol", Expect::Family("gpt")),
        ("chatgpt-web/gpt-5.6-sol", Expect::Family("gpt")),
        ("codex-auto-review", Expect::TaskLabel),
        ("", Expect::Unknown),
    ];

    #[test]
    fn bundled_registry_is_valid() {
        let r = ModelRegistry::bundled();
        assert!(!r.semver().is_empty());
        assert!(r.family("gpt").is_some());
        assert!(r.family("deepseek").is_some());
    }

    #[test]
    fn f0_samples_collapse_to_exactly_one_family() {
        let r = ModelRegistry::bundled();
        let mut families = BTreeSet::new();

        for (raw, expected) in F0_SAMPLES {
            let n = r.normalize(raw);
            match expected {
                Expect::Family(slug) => {
                    assert!(n.is_family(), "expected family for `{raw}`, got {:?}", n.disposition);
                    assert_eq!(n.family.as_deref(), Some(*slug), "raw `{raw}`");
                    families.insert(n.family.unwrap());
                }
                Expect::TaskLabel => {
                    assert!(n.is_task_label(), "`{raw}` must be a task label");
                    assert!(n.family.is_none());
                }
                Expect::Unknown => {
                    assert!(n.is_unknown(), "`{raw}` must be unknown");
                    assert!(n.family.is_none());
                }
            }
        }

        // Acceptance criterion: 9 raw values -> 1 family.
        assert_eq!(families.len(), 1);
        assert!(families.contains("gpt"));
    }

    #[test]
    fn naive_counting_would_have_been_wrong() {
        let r = ModelRegistry::bundled();
        let naive = F0_SAMPLES.len() as u32;
        let real = F0_SAMPLES
            .iter()
            .filter_map(|(raw, _)| r.normalize(raw).family)
            .collect::<BTreeSet<_>>()
            .len() as u32;
        assert_eq!(naive, 9);
        assert_eq!(real, 1);
    }

    #[test]
    fn client_prefix_is_extracted_not_invented() {
        let r = ModelRegistry::bundled();
        let n = r.normalize("chatgpt-web/gpt-6-sol");
        assert_eq!(n.client.as_deref(), Some("chatgpt-web"));
        assert_eq!(n.family.as_deref(), Some("gpt"));
        assert_eq!(n.raw, "chatgpt-web/gpt-6-sol");
    }

    #[test]
    fn task_label_is_never_a_family() {
        let r = ModelRegistry::bundled();
        for raw in ["codex-auto-review", "CODEX-AUTO-REVIEW", "  codex-auto-review  "] {
            let n = r.normalize(raw);
            assert!(n.is_task_label(), "raw `{raw}` -> {:?}", n.disposition);
            assert!(n.family.is_none());
        }
    }

    #[test]
    fn unknown_strings_are_not_guessed() {
        let r = ModelRegistry::bundled();
        for raw in ["totally-new-model", "gptq", "claude3", "anthropic.internal.v9", "!!"] {
            let n = r.normalize(raw);
            assert!(n.family.is_none(), "`{raw}` must not resolve to a family");
        }
    }

    #[test]
    fn prefix_rules_apply_only_when_declared() {
        let r = ModelRegistry::bundled();
        assert_eq!(r.normalize("gpt-99-experimental").family.as_deref(), Some("gpt"));
        assert_eq!(r.normalize("deepseek-r1").family.as_deref(), Some("deepseek"));
        assert_eq!(r.normalize("claude-sonnet-4-5").family.as_deref(), Some("claude"));
        assert_eq!(r.normalize("o3").family.as_deref(), Some("gpt"));
        // A model string that merely *contains* a known name must not match.
        assert!(r.normalize("my-gpt-wrapper").family.is_none());
    }

    #[test]
    fn registry_validation_rejects_broken_data() {
        let unknown_family = r#"{"semver":"1","families":[],"aliases":[
            {"raw":"x","match":"exact","disposition":{"kind":"family","family":"nope"}}]}"#;
        assert!(ModelRegistry::from_json(unknown_family).is_err());

        let duplicate = r#"{"semver":"1","families":[{"slug":"gpt","provider":"o","display_name":"G"}],
            "aliases":[
              {"raw":"x","match":"exact","disposition":{"kind":"family","family":"gpt"}},
              {"raw":"x","match":"exact","disposition":{"kind":"family","family":"gpt"}}]}"#;
        assert!(ModelRegistry::from_json(duplicate).is_err());

        let empty_semver = r#"{"semver":"  ","families":[],"aliases":[]}"#;
        assert!(ModelRegistry::from_json(empty_semver).is_err());

        let bad_unknown = r#"{"semver":"1","families":[],"aliases":[
            {"raw":"x","match":"exact","disposition":{"kind":"unknown"}}]}"#;
        assert!(ModelRegistry::from_json(bad_unknown).is_err());
    }

    #[test]
    fn normalization_records_registry_version() {
        let r = ModelRegistry::bundled();
        assert_eq!(r.normalize("gpt-6-sol").registry_semver, r.semver());
    }
}
