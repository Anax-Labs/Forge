//! `update_permissions` — create or update a contributor's role (§9.2, §4.6).
//!
//! Roles are stored in a per-contributor `PermissionAccount` PDA. Because
//! `init_if_needed` is forbidden (§11 #20), the handler distinguishes the
//! create and update paths explicitly: an empty account is created with the
//! guarded helper, an existing one is rewritten in place.

use anchor_lang::prelude::*;
use anchor_lang::{AccountDeserialize, AccountSerialize};

use crate::constants::PERM_SEED;
use crate::errors::ForgeError;
use crate::events::PermissionChanged;
use crate::init::create_pda;
use crate::state::permission::ROLE_ADMIN;
use crate::state::{PermissionAccount, RepositoryAccount};

/// Accounts for [`crate::forge_repository::update_permissions`].
#[event_cpi]
#[derive(Accounts)]
#[instruction(contributor: Pubkey, role: u8, expires_slot: u64)]
pub struct UpdatePermissions<'info> {
    /// Admin performing the change (MVP: the repository owner).
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Repository the permission applies to.
    #[account(mut)]
    pub repository: Account<'info, RepositoryAccount>,

    /// The permission PDA: `["perm", repository, contributor]`.
    /// CHECK: created or updated in the handler; address pinned by seeds.
    #[account(
        mut,
        seeds = [PERM_SEED, repository.key().as_ref(), contributor.as_ref()],
        bump
    )]
    pub permission: UncheckedAccount<'info>,

    /// System program (account creation on the first grant).
    pub system_program: Program<'info, System>,
}

/// Handler for `update_permissions`.
///
/// # Errors
/// - [`ForgeError::InvalidRole`] if `role` is not reader/writer/maintainer/admin.
/// - [`ForgeError::Unauthorized`] if the signer is not the repository owner.
pub fn handler(
    ctx: Context<UpdatePermissions>,
    contributor: Pubkey,
    role: u8,
    expires_slot: u64,
) -> Result<()> {
    require!(role <= ROLE_ADMIN, ForgeError::InvalidRole);

    let admin_key = ctx.accounts.admin.key();
    let repo_key = ctx.accounts.repository.key();
    // MVP: owner-only administration. Phase 9b can widen to admin-role holders.
    crate::auth::require_repo_owner(&ctx.accounts.repository, &admin_key)?;

    let slot = Clock::get()?.slot;
    let permission_info = ctx.accounts.permission.to_account_info();

    if permission_info.data_is_empty() {
        let state = PermissionAccount {
            repo: repo_key,
            contributor,
            role,
            granted_slot: slot,
            expires_slot,
            bump: ctx.bumps.permission,
        };
        let seeds: &[&[u8]] = &[
            PERM_SEED,
            repo_key.as_ref(),
            contributor.as_ref(),
            &[ctx.bumps.permission],
        ];
        create_pda(
            &ctx.accounts.admin.to_account_info(),
            &permission_info,
            seeds,
            8 + PermissionAccount::LEN,
            ForgeError::PermissionAlreadyExists,
            &state,
        )?;
    } else {
        let mut data = permission_info.try_borrow_mut_data()?;
        let mut permission = PermissionAccount::try_deserialize(&mut &data[..])?;
        require_keys_eq!(permission.repo, repo_key, ForgeError::Unauthorized);
        require_keys_eq!(
            permission.contributor,
            contributor,
            ForgeError::Unauthorized
        );
        permission.role = role;
        permission.granted_slot = slot;
        permission.expires_slot = expires_slot;
        let dst: &mut [u8] = &mut data;
        let mut cursor = std::io::Cursor::new(dst);
        permission.try_serialize(&mut cursor)?;
    }

    emit_cpi!(PermissionChanged {
        repository: repo_key,
        contributor,
        role,
        expires_slot,
        actor: admin_key,
        slot,
    });

    Ok(())
}
