use light_sdk::{
    light_account, light_accounts,
    merkle_context::{PackedAddressMerkleContext, PackedMerkleContext, PackedMerkleOutputContext},
    utils::{create_cpi_inputs_for_account_deletion, create_cpi_inputs_for_new_account},
    verify::verify,
    LightTraits,
};
use light_system_program::{invoke::processor::CompressedProof, sdk::CompressedCpiContext};

use anchor_lang::prelude::*;

declare_id!("B3XHqAM39stZu3Rxd6NTRZJtVUsPZ1G6NsvLPETjV3v1");

#[program]
pub mod compressed_aa {
    use super::*;
    use light_sdk::{
        address::derive_address_seed,
        compressed_account::{
            input_compressed_account, new_compressed_account, output_compressed_account,
        },
        merkle_context::unpack_address_merkle_context,
        utils::create_cpi_inputs_for_account_update,
    };

    #[allow(clippy::too_many_arguments)]
    pub fn create_hotkey<'info>(
        ctx: Context<'_, '_, '_, 'info, CompressedHotkeys<'info>>,
        proof: CompressedProof,
        merkle_output_context: PackedMerkleOutputContext,
        address_merkle_context: PackedAddressMerkleContext,
        address_merkle_tree_root_index: u16,
        wallet: Pubkey,
        controller: Pubkey,
        cpi_context: Option<CompressedCpiContext>,
    ) -> Result<()> {
        let unpacked_address_merkle_context =
            unpack_address_merkle_context(address_merkle_context, ctx.remaining_accounts);
        
        let address_seed = derive_address_seed(
            &[
                wallet.key().to_bytes().as_slice(),
                controller.key().to_bytes().as_slice(),
            ],
            &crate::ID,
            &unpacked_address_merkle_context,
        );

        let hotkey = Hotkey{
            controller,
            wallet
        };

        let (compressed_account, new_address_params) = new_compressed_account(
            &hotkey,
            &address_seed,
            &crate::ID,
            &merkle_output_context,
            &address_merkle_context,
            address_merkle_tree_root_index,
            ctx.remaining_accounts,
        )?;

        let signer_seed = b"cpi_signer".as_slice();
        let (_, bump) = Pubkey::find_program_address(&[signer_seed], &ctx.accounts.self_program.key());
        let signer_seeds = [signer_seed, &[bump]];

        let inputs = create_cpi_inputs_for_new_account(
            proof,
            new_address_params,
            compressed_account,
            &signer_seeds,
            cpi_context,
        );

        verify(ctx, &inputs, &[&signer_seeds])?;

        Ok(())
    }
}

#[light_account]
#[derive(Debug)]
pub struct Hotkey {
    #[truncate]
    pub wallet: Pubkey,
    #[truncate]
    pub controller: Pubkey,
}

#[light_accounts]
#[derive(Accounts, LightTraits)]
pub struct CompressedHotkeys<'info> {
    #[account(mut)]
    #[fee_payer]
    pub signer: Signer<'info>,
    #[self_program]
    pub self_program: Program<'info, crate::program::CompressedAa>,
    /// CHECK: Checked in light-system-program.
    /// Why do we have to add it manually? light_accounts could theoretically handle this?
    #[authority]
    pub cpi_signer: AccountInfo<'info>,
}
