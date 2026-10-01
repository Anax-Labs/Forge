//! Guarded manual PDA initialization.
//!
//! Anchor's `#[account(init)]` gives robust uniqueness but reports a duplicate
//! with the system program's generic `AccountAlreadyInUse`. Forge needs
//! protocol-specific errors so clients can distinguish "this name is taken"
//! from other initialization failures. This helper performs the same guarded
//! creation manually and returns the caller's error on a non-empty account.
//!
//! Safety properties (§11):
//! - The account address is pinned by a `seeds` + `bump` constraint on the
//!   caller's `UncheckedAccount`, so it cannot be substituted.
//! - Reinitialization is rejected by the `data_is_empty()` guard (forbidden
//!   `init_if_needed` is not used).
//! - Ownership is set to this program by `create_account`; the account is
//!   rent-exempt and exactly `space` bytes.

use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_lang::{prelude::*, AccountSerialize};

use crate::errors::ForgeError;

/// Creates a PDA if it is empty, then serializes `state` into it.
///
/// `seeds` must include the canonical bump as its final element.
///
/// # Errors
/// Returns `already_exists` when the account already holds data, or propagates
/// system-program / serialization failures.
pub(crate) fn create_pda<'info, T: AccountSerialize>(
    payer: &AccountInfo<'info>,
    account: &AccountInfo<'info>,
    seeds: &[&[u8]],
    space: usize,
    already_exists: ForgeError,
    state: &T,
) -> Result<()> {
    if !account.data_is_empty() {
        return Err(already_exists.into());
    }

    let rent = Rent::get()?.minimum_balance(space);
    create_account(
        CpiContext::new_with_signer(
            anchor_lang::system_program::ID,
            CreateAccount {
                from: payer.clone(),
                to: account.clone(),
            },
            &[seeds],
        ),
        rent,
        u64::try_from(space).expect("account space fits in u64"),
        &crate::ID,
    )?;

    let mut data = account.try_borrow_mut_data()?;
    let dst: &mut [u8] = &mut data;
    let mut cursor = std::io::Cursor::new(dst);
    state.try_serialize(&mut cursor)?;
    Ok(())
}
