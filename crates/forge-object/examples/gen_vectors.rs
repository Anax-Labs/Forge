//! Regenerates the protocol golden vectors.
//!
//! ```text
//! cargo run -p forge-object --example gen_vectors > tests/vectors/golden.json
//! ```
//!
//! The output is a reviewable protocol artifact; changing it is a deliberate
//! decision (see `tests/golden.rs`).

#[path = "../tests/common/mod.rs"]
mod common;

fn main() {
    let value = common::compute_json();
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("serialize golden vectors")
    );
}
