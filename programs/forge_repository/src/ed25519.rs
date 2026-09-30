//! Ed25519 signature introspection via the Instructions sysvar (§9.4).
//!
//! Clients prepend the native Ed25519 verify instruction; this module parses it
//! and asserts the signed message and public key match the expected commit
//! attestation. Reused by branch-update instructions in Phase 5.

use crate::constants::ED25519_PROGRAM_ID;
use crate::errors::ForgeError;
use anchor_lang::prelude::*;
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};

/// Serialized size of one `Ed25519SignatureOffsets` struct in the instruction data.
const SIGNATURE_OFFSETS_SERIALIZED_SIZE: usize = 14;
/// Byte offset where signature/public-key/message bytes begin.
const DATA_START: usize = 2 + SIGNATURE_OFFSETS_SERIALIZED_SIZE;

/// Requires the instruction immediately before the current one to be a valid
/// Ed25519 native verify for `(expected_author, expected_message)`, and that
/// the transaction contains **exactly one** Ed25519 instruction (§9.4).
///
/// # Errors
/// Returns [`ForgeError::InvalidEd25519Instruction`] or
/// [`ForgeError::BadSignature`] when introspection or parsing fails.
pub fn verify_ed25519_instruction_preceding(
    instructions_sysvar: &AccountInfo,
    expected_author: &Pubkey,
    expected_message: &[u8],
) -> Result<()> {
    require!(
        !expected_message.is_empty(),
        ForgeError::InvalidEd25519Instruction
    );

    let current_index = usize::from(
        load_current_index_checked(instructions_sysvar)
            .map_err(|_| error!(ForgeError::InvalidEd25519Instruction))?,
    );
    require!(current_index > 0, ForgeError::InvalidEd25519Instruction);

    let mut ed25519_count: u8 = 0;
    for i in 0..=current_index {
        let ix = load_instruction_at_checked(i, instructions_sysvar)
            .map_err(|_| error!(ForgeError::InvalidEd25519Instruction))?;
        if ix.program_id.to_bytes() == ED25519_PROGRAM_ID.to_bytes() {
            ed25519_count = ed25519_count
                .checked_add(1)
                .ok_or(ForgeError::MathOverflow)?;
        }
    }
    require_eq!(ed25519_count, 1, ForgeError::InvalidEd25519Instruction);

    let ed25519_ix = load_instruction_at_checked(
        current_index
            .checked_sub(1)
            .ok_or(ForgeError::InvalidEd25519Instruction)?,
        instructions_sysvar,
    )
    .map_err(|_| error!(ForgeError::InvalidEd25519Instruction))?;
    require!(
        ed25519_ix.program_id.to_bytes() == ED25519_PROGRAM_ID.to_bytes(),
        ForgeError::InvalidEd25519Instruction
    );

    parse_ed25519_instruction_data(&ed25519_ix.data, expected_author, expected_message)
}

fn parse_ed25519_instruction_data(
    data: &[u8],
    expected_author: &Pubkey,
    expected_message: &[u8],
) -> Result<()> {
    require!(
        data.len() >= DATA_START,
        ForgeError::InvalidEd25519Instruction
    );
    require_eq!(data[0], 1, ForgeError::InvalidEd25519Instruction);
    require_eq!(data[1], 0, ForgeError::InvalidEd25519Instruction);

    let signature_offset = usize::from(u16::from_le_bytes([data[2], data[3]]));
    let signature_ix_index = u16::from_le_bytes([data[4], data[5]]);
    let public_key_offset = usize::from(u16::from_le_bytes([data[6], data[7]]));
    let public_key_ix_index = u16::from_le_bytes([data[8], data[9]]);
    let message_offset = usize::from(u16::from_le_bytes([data[10], data[11]]));
    let message_size = usize::from(u16::from_le_bytes([data[12], data[13]]));
    let message_ix_index = u16::from_le_bytes([data[14], data[15]]);

    require_eq!(signature_ix_index, 0, ForgeError::InvalidEd25519Instruction);
    require_eq!(
        public_key_ix_index,
        0,
        ForgeError::InvalidEd25519Instruction
    );
    require_eq!(message_ix_index, 0, ForgeError::InvalidEd25519Instruction);

    require!(
        signature_offset >= DATA_START
            && public_key_offset >= DATA_START
            && message_offset >= DATA_START,
        ForgeError::InvalidEd25519Instruction
    );
    require!(message_size > 0, ForgeError::InvalidEd25519Instruction);
    require_eq!(
        message_size,
        expected_message.len(),
        ForgeError::BadSignature
    );

    let signature_end = signature_offset
        .checked_add(64)
        .ok_or(ForgeError::MathOverflow)?;
    let public_key_end = public_key_offset
        .checked_add(32)
        .ok_or(ForgeError::MathOverflow)?;
    let message_end = message_offset
        .checked_add(message_size)
        .ok_or(ForgeError::MathOverflow)?;
    require!(
        signature_end <= data.len() && public_key_end <= data.len() && message_end <= data.len(),
        ForgeError::InvalidEd25519Instruction
    );

    let signature = &data[signature_offset..signature_end];
    let public_key = &data[public_key_offset..public_key_end];
    let message = &data[message_offset..message_end];

    let author_bytes = expected_author.to_bytes();
    require!(
        public_key == author_bytes.as_slice(),
        ForgeError::BadSignature
    );
    require!(message == expected_message, ForgeError::BadSignature);

    require!(signature.iter().any(|b| *b != 0), ForgeError::BadSignature);

    Ok(())
}
