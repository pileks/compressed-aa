// #![cfg(feature = "test-sbf")]

use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
use compressed_aa::Hotkey;
use light_sdk::address::derive_address_seed;
use light_sdk::merkle_context::{
    pack_address_merkle_context, pack_merkle_context, pack_merkle_output_context,
    AddressMerkleContext, MerkleOutputContext, PackedAddressMerkleContext, RemainingAccounts,
};
use light_system_program::sdk::address::derive_address;
use light_system_program::sdk::compressed_account::CompressedAccountWithMerkleContext;
use light_test_utils::indexer::test_indexer::TestIndexer;
use light_test_utils::rpc::ProgramTestRpcConnection;
use light_test_utils::test_env::{setup_test_programs_with_accounts, EnvAccounts};
use light_test_utils::{indexer::Indexer, rpc::rpc_connection::RpcConnection};
use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;

fn find_cpi_signer() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"cpi_signer"], &compressed_aa::ID)
}

fn instruction_accounts(
    payer: &Keypair,
    account_compression_authority: &Pubkey,
    registered_program_pda: &Pubkey,
    cpi_signer: &Pubkey,
) -> compressed_aa::accounts::CompressedHotkeys {
    compressed_aa::accounts::CompressedHotkeys {
        signer: payer.pubkey(),
        light_system_program: light_system_program::ID,
        account_compression_program: account_compression::ID,
        account_compression_authority: *account_compression_authority,
        registered_program_pda: *registered_program_pda,
        noop_program: Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        self_program: compressed_aa::ID,
        cpi_signer: *cpi_signer,
        system_program: solana_sdk::system_program::id(),
    }
}

#[tokio::test]
async fn test_compressed_aa() {
    let (mut rpc, env) = setup_test_programs_with_accounts(Some(vec![(
        String::from("compressed_aa"),
        compressed_aa::ID,
    )]))
    .await;

    let payer = rpc.get_payer().insecure_clone();

    let mut test_indexer: TestIndexer<ProgramTestRpcConnection> =
        TestIndexer::init_from_env(&payer, &env, true, true).await;

    let wallet = Keypair::new();
    let controller = Keypair::new();

    let mut remaining_accounts = RemainingAccounts::default();

    let address_merkle_context = AddressMerkleContext {
        address_merkle_tree_pubkey: env.address_merkle_tree_pubkey,
        address_queue_pubkey: env.address_merkle_tree_queue_pubkey,
    };

    let address_seed = derive_address_seed(
        &[
            wallet.pubkey().to_bytes().as_slice(),
            controller.pubkey().to_bytes().as_slice(),
        ],
        &compressed_aa::ID,
        &address_merkle_context,
    );

    let address = derive_address(&env.address_merkle_tree_pubkey, &address_seed).unwrap();

    let address_merkle_context =
        pack_address_merkle_context(address_merkle_context, &mut remaining_accounts);

    let account_compression_authority =
        light_system_program::utils::get_cpi_authority_pda(&light_system_program::ID);

    let (registered_program_pda, _) = Pubkey::find_program_address(
        &[light_system_program::ID.to_bytes().as_slice()],
        &account_compression::ID,
    );

    create_hotkey(
        wallet.pubkey(),
        controller.pubkey(),
        &mut rpc,
        &mut test_indexer,
        &env,
        &mut remaining_accounts,
        &payer,
        &address,
        &address_merkle_context,
        &account_compression_authority,
        &registered_program_pda,
    )
    .await;

    // Check that it was created correctly.
    let compressed_accounts = test_indexer.get_compressed_accounts_by_owner(&compressed_aa::ID);
    assert_eq!(compressed_accounts.len(), 1);

    let compressed_account = &compressed_accounts[0];
    let hotkey = &compressed_account
        .compressed_account
        .data
        .as_ref()
        .unwrap()
        .data;

    let hotkey = Hotkey::deserialize(&mut &hotkey[..]).unwrap();
    assert_eq!(hotkey.controller, controller.pubkey());
    assert_eq!(hotkey.wallet, wallet.pubkey());
}

#[allow(clippy::too_many_arguments)]
async fn create_hotkey<R: RpcConnection>(
    wallet: Pubkey,
    controller: Pubkey,
    rpc: &mut R,
    test_indexer: &mut TestIndexer<R>,
    env: &EnvAccounts,
    remaining_accounts: &mut RemainingAccounts,
    payer: &Keypair,
    address: &[u8; 32],
    address_merkle_context: &PackedAddressMerkleContext,
    account_compression_authority: &Pubkey,
    registered_program_pda: &Pubkey,
) {
    let rpc_result = test_indexer
        .create_proof_for_compressed_accounts(
            None,
            None,
            Some(&[*address]),
            Some(vec![env.address_merkle_tree_pubkey]),
            rpc,
        )
        .await;

    let merkle_output_context = MerkleOutputContext {
        merkle_tree_pubkey: env.merkle_tree_pubkey,
    };

    let merkle_output_context =
        pack_merkle_output_context(merkle_output_context, remaining_accounts);

    let instruction_data = compressed_aa::instruction::CreateHotkey {
        proof: rpc_result.proof,
        merkle_output_context,
        address_merkle_context: *address_merkle_context,
        address_merkle_tree_root_index: rpc_result.address_root_indices[0],
        controller,
        wallet,
        cpi_context: None,
    };

    let (cpi_signer, _) = find_cpi_signer();
    
    let accounts = instruction_accounts(
        payer,
        account_compression_authority,
        registered_program_pda,
        &cpi_signer,
    );

    let remaining_accounts = remaining_accounts.to_account_metas();

    let instruction = Instruction {
        program_id: compressed_aa::ID,
        accounts: [accounts.to_account_metas(Some(true)), remaining_accounts].concat(),
        data: instruction_data.data(),
    };

    let event_or_err = rpc
        .create_and_send_transaction_with_event(&[instruction], &payer.pubkey(), &[payer], None)
        .await;

    println!("AAAA {:?}", event_or_err);

    match event_or_err {
        Ok(event) => test_indexer.add_compressed_accounts_with_token_data(&event.unwrap().0),
        Err(e) => println!("{}", e),
    };
}
