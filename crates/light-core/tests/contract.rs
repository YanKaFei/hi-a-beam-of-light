//! Contract tests: the committed fixtures must match what the Rust core produces.
//!
//! The fixtures in `packages/protocol/fixtures/` are the shared source of truth
//! between this crate and every other platform. The TypeScript reference
//! implementation is validated against them in
//! `packages/protocol/test/conformance.test.mjs`.
//!
//! Together the two suites pin both implementations to the same numbers, so a
//! change on either side that is not reflected in the other fails loudly.
//!
//! These tests are read-only: regenerate with
//! `cargo run -p light-core --example emit_contract`.

use light_core::evidence::ResponseProof;
use light_core::registry::Disposition;
use light_core::{compute_light_state, knowledge_title, light_level, protocol, transmission_title};
use light_core::{EvidencePolicy, LightLevel, ModelRegistry};
use serde_json::Value;
use std::path::PathBuf;

fn protocol_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir.push("packages");
    dir.push("protocol");
    dir
}

fn load(relative: &str) -> Value {
    let path = protocol_dir().join(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("invalid json in {}: {e}", path.display()))
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
fn titles_fixture_matches_the_core() {
    let fixture = load("fixtures/titles.json");

    for row in fixture["knowledge"].as_array().unwrap() {
        let families = row["families"].as_u64().unwrap() as u32;
        assert_eq!(knowledge_title(families).slug(), row["slug"].as_str().unwrap());
        assert_eq!(knowledge_title(families).display_zh(), row["zh"].as_str().unwrap());
    }

    for row in fixture["transmission"].as_array().unwrap() {
        let nodes = row["nodes"].as_u64().unwrap() as u32;
        assert_eq!(transmission_title(nodes).slug(), row["slug"].as_str().unwrap());
        assert_eq!(transmission_title(nodes).display_zh(), row["zh"].as_str().unwrap());
    }

    for row in fixture["levels"].as_array().unwrap() {
        let families = row["families"].as_u64().unwrap() as u32;
        assert_eq!(level_name(light_level(families)), row["level"].as_str().unwrap());
    }
}

#[test]
fn registry_fixture_matches_the_core() {
    let fixture = load("fixtures/registry_normalization.json");
    let registry = ModelRegistry::bundled();
    assert_eq!(registry.semver(), fixture["registry_semver"].as_str().unwrap());

    for case in fixture["cases"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let got = registry.normalize(raw);

        let expected_kind = case["disposition"]["kind"].as_str().unwrap();
        match &got.disposition {
            Disposition::Family { .. } => assert_eq!(expected_kind, "family", "raw {raw:?}"),
            Disposition::TaskLabel { .. } => assert_eq!(expected_kind, "task_label", "raw {raw:?}"),
            Disposition::Unknown => assert_eq!(expected_kind, "unknown", "raw {raw:?}"),
        }
        assert_eq!(
            got.family.as_deref(),
            case["family"].as_str(),
            "family mismatch for raw {raw:?}"
        );
        assert_eq!(
            got.client.as_deref(),
            case["client"].as_str(),
            "client mismatch for raw {raw:?}"
        );
    }
}

#[test]
fn evidence_fixture_matches_the_core() {
    let fixture = load("fixtures/evidence_decisions.json");
    let policy = EvidencePolicy::v1();

    for case in fixture["cases"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let output = case["output_items"].as_u64().unwrap() as u32;
        let turns = case["completed_turns"].as_u64().unwrap() as u32;
        let proof = ResponseProof::from_counts(output, turns);
        let got = policy.evaluate(raw, &proof);
        let expected = case["decision"].as_str().unwrap();

        let actual = match &got {
            light_core::EvidenceDecision::Countable { .. } => "countable",
            light_core::EvidenceDecision::Observable { .. } => "observable",
            light_core::EvidenceDecision::Rejected { .. } => "rejected",
        };
        assert_eq!(actual, expected, "raw {raw:?}");

        if expected == "countable" {
            assert_eq!(
                got.family(),
                Some(case["detail"]["family"].as_str().unwrap()),
                "raw {raw:?}"
            );
        }
    }
}

#[test]
fn light_state_fixture_matches_the_core() {
    let fixture = load("fixtures/light_state.json");

    for case in fixture["cases"].as_array().unwrap() {
        let verified = case["verified_families"].as_u64().unwrap() as u32;
        let attested = case["attested_families"].as_u64().unwrap() as u32;
        let received = case["received_glow"].as_u64().unwrap() as u32;
        let nodes = case["activated_nodes"].as_u64().unwrap() as u32;

        let state = compute_light_state(verified, attested, received, nodes);
        let expected = &case["expected"];
        assert_eq!(level_name(state.level), expected["level"].as_str().unwrap());
        assert_eq!(
            state.knowledge_title.slug(),
            expected["knowledge_title"].as_str().unwrap()
        );
        assert_eq!(
            state.transmission_title.slug(),
            expected["transmission_title"].as_str().unwrap()
        );
    }
}

#[test]
fn committed_schemas_match_the_core() {
    // Regenerating must be a no-op. If this fails, run emit_contract and commit.
    let generated = protocol::exported_schemas();
    let dir = protocol_dir().join("schemas");
    let mut seen = 0;

    for (name, expected) in &generated {
        let path = dir.join(format!("{name}.schema.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing schema {}: {e}", path.display()));
        let committed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            &committed, expected,
            "schema `{name}` is stale; run `cargo run -p light-core --example emit_contract`"
        );
        seen += 1;
    }
    assert_eq!(seen, generated.len());
}

#[test]
fn fixtures_contain_no_personal_data() {
    // The fixtures are committed to a public repository. Guard against a real
    // machine path or content sneaking in during a regeneration.
    for file in [
        "fixtures/titles.json",
        "fixtures/registry_normalization.json",
        "fixtures/light_state.json",
        "fixtures/evidence_decisions.json",
    ] {
        let text = std::fs::read_to_string(protocol_dir().join(file)).unwrap();
        for needle in ["/Users/", "SECRET", "sk-", "thread-id", "first_user_message"] {
            assert!(
                !text.contains(needle),
                "`{file}` must not contain `{needle}`"
            );
        }
    }
}
