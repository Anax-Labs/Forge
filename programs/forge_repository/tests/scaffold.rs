//! Phase 1 scaffold smoke test.
//!
//! Verifies the program crate is linkable and its declared program ID is a real
//! (non-default) address. Real instruction/account tests land in Phases 3–5.

use anchor_lang::prelude::Pubkey;

#[test]
fn declared_program_id_is_set() {
    let id = forge_repository::id();
    assert_ne!(id, Pubkey::default(), "declare_id! was not set");
}
