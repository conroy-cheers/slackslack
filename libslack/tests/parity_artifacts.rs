use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct ReleaseGates {
    schema_version: String,
    statuses: BTreeMap<String, StatusRule>,
    required_layers_for_release_ready: Vec<String>,
    canary_requirement_for_release_ready: String,
    release_gated_families: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct StatusRule {
    description: String,
    requires_evidence_kind_any_of: Vec<String>,
    allows_release_ready: bool,
}

#[derive(Debug, Deserialize)]
struct CoverageMatrix {
    schema_version: String,
    catalog_source: String,
    release_gate_policy: String,
    families: Vec<Family>,
}

#[derive(Debug, Deserialize)]
struct Family {
    id: String,
    release_gated: bool,
    interfaces: Vec<Interface>,
}

#[derive(Debug, Deserialize)]
struct Interface {
    name: String,
    status: String,
    source: String,
    classification: String,
    release_ready: bool,
    canary_eligible: bool,
    coverage: Coverage,
    evidence: Vec<Evidence>,
}

#[derive(Debug, Deserialize)]
struct Coverage {
    unit: bool,
    schema: bool,
    replay: bool,
    workflow: bool,
    canary: bool,
}

#[derive(Debug, Deserialize)]
struct Evidence {
    kind: String,
    path: String,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

#[test]
fn coverage_matrix_has_required_families_and_valid_statuses() {
    let root = workspace_root();
    let matrix: CoverageMatrix = read_json(&root.join("docs/parity/coverage-matrix.json"));
    let gates: ReleaseGates = read_json(&root.join("docs/parity/release-gates.json"));

    assert_eq!(matrix.schema_version, "1.0");
    assert_eq!(gates.schema_version, "1.0");
    assert_eq!(matrix.catalog_source, "docs/parity/catalog-current.md");
    assert_eq!(matrix.release_gate_policy, "docs/parity/release-gates.json");
    assert_eq!(
        gates.canary_requirement_for_release_ready,
        "recommended_not_required"
    );
    assert_eq!(
        gates.required_layers_for_release_ready,
        vec!["unit", "replay"]
    );

    let family_ids: BTreeSet<_> = matrix.families.iter().map(|f| f.id.as_str()).collect();
    for required in [
        "bootstrap_session",
        "conversations_workspace_metadata",
        "messages_threads_reactions_pins",
        "users_presence_profiles",
        "search",
        "files_media_metadata",
        "excluded_surfaces",
    ] {
        assert!(family_ids.contains(required), "missing family {required}");
    }

    for family in &matrix.families {
        assert!(
            !family.interfaces.is_empty(),
            "family {} has no interfaces",
            family.id
        );
        for interface in &family.interfaces {
            assert!(
                gates.statuses.contains_key(&interface.status),
                "unknown status {}",
                interface.status
            );
            assert!(!interface.name.is_empty(), "interface missing name");
            assert!(
                !interface.source.is_empty(),
                "{} missing source",
                interface.name
            );
            assert!(
                !interface.classification.is_empty(),
                "{} missing classification",
                interface.name
            );
            assert!(
                !interface.evidence.is_empty(),
                "{} missing evidence",
                interface.name
            );
        }
    }
}

#[test]
fn status_semantics_and_evidence_paths_are_enforced() {
    let root = workspace_root();
    let matrix: CoverageMatrix = read_json(&root.join("docs/parity/coverage-matrix.json"));
    let gates: ReleaseGates = read_json(&root.join("docs/parity/release-gates.json"));

    for family in &matrix.families {
        for interface in &family.interfaces {
            let rule = gates.statuses.get(&interface.status).unwrap();
            let allowed: BTreeSet<_> = rule
                .requires_evidence_kind_any_of
                .iter()
                .map(String::as_str)
                .collect();
            assert!(
                interface
                    .evidence
                    .iter()
                    .any(|ev| allowed.contains(ev.kind.as_str())),
                "{} has no evidence kind allowed for status {}",
                interface.name,
                interface.status
            );
            for evidence in &interface.evidence {
                let path = root.join(&evidence.path);
                assert!(
                    path.exists(),
                    "evidence path missing for {}: {}",
                    interface.name,
                    path.display()
                );
            }

            if interface.status == "implemented_observed" {
                assert!(
                    interface.evidence.iter().any(|ev| matches!(
                        ev.kind.as_str(),
                        "observed_capture" | "replay_fixture" | "desktop_local_state"
                    )),
                    "{} needs concrete observation evidence",
                    interface.name
                );
            }
            if interface.status == "implemented_seeded" || interface.status == "decode_covered" {
                assert!(
                    !interface.release_ready,
                    "{} cannot be release-ready while status is {}",
                    interface.name, interface.status
                );
            }
            if interface.status == "excluded" {
                assert!(
                    !family.release_gated,
                    "excluded family {} must not be release-gated",
                    family.id
                );
            }
            assert!(
                !rule.description.is_empty(),
                "status rule {} missing description",
                interface.status
            );
        }
    }
}

#[test]
fn release_gate_rules_are_machine_checkable() {
    let root = workspace_root();
    let matrix: CoverageMatrix = read_json(&root.join("docs/parity/coverage-matrix.json"));
    let gates: ReleaseGates = read_json(&root.join("docs/parity/release-gates.json"));

    let release_families: BTreeSet<_> = gates
        .release_gated_families
        .iter()
        .map(String::as_str)
        .collect();
    for family in &matrix.families {
        assert_eq!(
            family.release_gated,
            release_families.contains(family.id.as_str()),
            "family {} release-gated mismatch",
            family.id
        );
        for interface in &family.interfaces {
            if interface.release_ready {
                let rule = gates.statuses.get(&interface.status).unwrap();
                assert!(
                    family.release_gated,
                    "{} must belong to release-gated family",
                    interface.name
                );
                assert!(
                    rule.allows_release_ready,
                    "status {} cannot be release-ready",
                    interface.status
                );
                assert!(
                    interface.coverage.unit,
                    "{} missing unit coverage",
                    interface.name
                );
                assert!(
                    interface.coverage.replay,
                    "{} missing replay coverage",
                    interface.name
                );
            }
            if interface.coverage.canary {
                assert!(
                    interface.canary_eligible,
                    "{} has canary coverage but is not canary-eligible",
                    interface.name
                );
                assert!(
                    family.release_gated,
                    "canary coverage should be reserved for release-gated families"
                );
            }
            if interface.status == "implemented_observed" && interface.release_ready {
                assert!(
                    interface.coverage.replay,
                    "observed release-ready interface {} needs replay coverage",
                    interface.name
                );
            }
            if interface.coverage.workflow {
                assert!(
                    interface.coverage.unit,
                    "workflow coverage without unit coverage for {} is suspicious",
                    interface.name
                );
            }
            if interface.coverage.schema {
                assert!(
                    matches!(
                        interface.status.as_str(),
                        "implemented_seeded" | "implemented_observed" | "decode_covered"
                    ),
                    "schema coverage on unsupported status for {}",
                    interface.name
                );
            }
        }
    }
}

#[test]
fn committed_catalog_and_matrix_stay_in_sync() {
    let root = workspace_root();
    let matrix: CoverageMatrix = read_json(&root.join("docs/parity/coverage-matrix.json"));
    let catalog =
        fs::read_to_string(root.join("docs/parity/catalog-current.md")).expect("catalog markdown");

    for heading in [
        "## Family: bootstrap / session",
        "## Family: conversations / workspace metadata",
        "## Family: messages / threads / reactions / pins",
        "## Family: users / presence / profiles",
        "## Family: search",
        "## Family: files / media metadata",
        "## Family: excluded for this phase",
    ] {
        assert!(
            catalog.contains(heading),
            "catalog missing heading {heading}"
        );
    }

    for family in &matrix.families {
        if family.id == "excluded_surfaces" {
            continue;
        }
        for interface in &family.interfaces {
            let status_token = interface.status.replace('_', "-");
            let matching_line = catalog.lines().find(|line| {
                line.contains(&format!("`{}`", interface.name))
                    || line.contains(&interface.name)
                    || interface
                        .name
                        .split('/')
                        .all(|part| line.contains(part.trim()))
            });
            let line = matching_line.unwrap_or_else(|| {
                panic!(
                    "catalog missing interface {} from family {}",
                    interface.name, family.id
                )
            });
            assert!(
                line.contains(&status_token),
                "catalog row for {} missing status {}",
                interface.name,
                status_token
            );
            assert!(
                line.contains(&interface.classification),
                "catalog row for {} missing classification {}",
                interface.name,
                interface.classification
            );
        }
    }
}

#[test]
fn parity_supporting_artifacts_exist() {
    let root = workspace_root();
    for rel in [
        "docs/parity/README.md",
        "docs/parity/catalog-current.md",
        "docs/parity/scope-policy.md",
        "docs/parity/seed-observations.md",
        "docs/parity/live-canary-policy.md",
        "docs/parity/fixtures/README.md",
        "docs/parity/drift-report.example.json",
    ] {
        assert!(root.join(rel).exists(), "missing artifact: {rel}");
    }

    let drift_report: serde_json::Value =
        read_json(&root.join("docs/parity/drift-report.example.json"));
    assert_eq!(drift_report["schema_version"], "1.0");
    assert!(drift_report["entries"].is_array());
}
