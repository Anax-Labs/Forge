//! Checked-in golden-vector regression test.
//!
//! The expected values live in the repository (not the crate) at
//! `tests/vectors/golden.json`, so they are reviewable as protocol artifacts.
//! Regenerate intentionally with:
//!
//! ```text
//! cargo run -p forge-object --example gen_vectors > tests/vectors/golden.json
//! ```
//!
//! Any diff here means the canonical encoding changed and must be a deliberate
//! protocol decision.

mod common;

#[test]
fn golden_vectors_match() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/golden.json"
    );
    let raw = std::fs::read_to_string(path).expect("read tests/vectors/golden.json");
    let expected: serde_json::Value = serde_json::from_str(&raw).expect("parse golden.json");
    let actual = common::compute_json();

    assert!(
        expected == actual,
        "golden vectors differ\n--- expected ---\n{}\n--- actual ---\n{}\n\
         If this change is intentional, regenerate with:\n\
         cargo run -p forge-object --example gen_vectors > tests/vectors/golden.json",
        serde_json::to_string_pretty(&expected).expect("pretty expected"),
        serde_json::to_string_pretty(&actual).expect("pretty actual"),
    );
}
