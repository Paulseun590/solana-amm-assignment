use {
    anchor_spl::associated_token,
    litesvm::LiteSVM,
    litesvm_token::CreateMint,
    solana_keypair::Keypair,
    solana_message::{Instruction, Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

mod ix_handlers;
use ix_handlers::*;

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> litesvm::types::TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

fn token_amount(svm: &LiteSVM, address: &Pubkey) -> u64 {
    let account = svm
        .get_account(address)
        .expect("Token account should exist");

    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

// Setup function to initialize LiteSVM and create a payer keypair
fn setup() -> (
    LiteSVM,
    Keypair,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let program_id = amm_video::id();
    let payer = Keypair::new();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/amm_video.so");
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

    // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
    // This done using litesvm-token's CreateMint utility which creates the mint in the LiteSVM environment
    let mint_x = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();

    let mint_y = CreateMint::new(&mut svm, &payer)
        .decimals(6)
        .authority(&payer.pubkey())
        .send()
        .unwrap();

    let config =
        Pubkey::find_program_address(&[b"config", &123u64.to_le_bytes()], &amm_video::id()).0;
    let mint_lp = Pubkey::find_program_address(&[b"lp", config.as_ref()], &amm_video::id()).0;

    // Derive the PDA for the vault associated token account using the config PDA and Mint A
    let vault_x = associated_token::get_associated_token_address(&config, &mint_x);
    let vault_y = associated_token::get_associated_token_address(&config, &mint_y);

    (
        svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y,
    )
}

#[test]
fn test_initialize() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let instruction = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 30,
    );
    let res = send(&mut svm, &[instruction], &payer, &[&payer]);
    assert!(res.is_ok());
}

#[test]
pub fn test_deposit() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 30,
    );

    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]);
    assert!(res.is_ok());
}

#[test]
pub fn test_withdraw() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 30,
    );

    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let withdraw_ix = create_withdraw_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );
    let res = send(
        &mut svm,
        &[init_ix, deposit_ix, withdraw_ix],
        &payer,
        &[&payer],
    );
    assert!(res.is_ok());
}

#[test]
pub fn test_swap() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();
    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 30,
    );

    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let swap_ix = create_swap_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, true, 10_000_000,
        5_000_000,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix, swap_ix], &payer, &[&payer]);
    assert!(res.is_ok());

    let treasury_authority =
        Pubkey::find_program_address(&[b"treasury", config.as_ref()], &amm_video::id()).0;

    let treasury_x = associated_token::get_associated_token_address(&treasury_authority, &mint_x);

    let treasury_y = associated_token::get_associated_token_address(&treasury_authority, &mint_y);

    // 30 bps of 10,000,000 = 30,000.
    assert_eq!(token_amount(&svm, &treasury_x), 30_000);

    // Only the net input (9,970,000) should enter the pool.
    assert_eq!(token_amount(&svm, &vault_x), 209_970_000);

    // No X-to-Y swap fee should be deposited in the Y treasury.
    assert_eq!(token_amount(&svm, &treasury_y), 0);

    // The swap must also reduce the Y reserve.
    assert!(token_amount(&svm, &vault_y) < 200_000_000);
}

#[test]
pub fn test_swap_y_to_x() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 30,
    );

    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    let swap_ix = create_swap_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, false, 10_000_000,
        5_000_000,
    );

    let res = send(&mut svm, &[init_ix, deposit_ix, swap_ix], &payer, &[&payer]);
    assert!(res.is_ok());

    let treasury_authority =
        Pubkey::find_program_address(&[b"treasury", config.as_ref()], &amm_video::id()).0;

    let treasury_x = associated_token::get_associated_token_address(&treasury_authority, &mint_x);

    let treasury_y = associated_token::get_associated_token_address(&treasury_authority, &mint_y);

    assert_eq!(token_amount(&svm, &treasury_y), 30_000);
    assert_eq!(token_amount(&svm, &treasury_x), 0);
    assert_eq!(token_amount(&svm, &vault_y), 209_970_000);
    assert!(token_amount(&svm, &vault_x) < 200_000_000);
}

#[test]
fn test_invalid_fee_rejected() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 10_000,
    );

    let res = send(&mut svm, &[init_ix], &payer, &[&payer]);
    assert!(
        res.is_err(),
        "A fee of 10,000 basis points must be rejected"
    );
}

#[test]
fn test_zero_amount_swap_rejected() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 30,
    );
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    assert!(send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]).is_ok());

    let swap_ix = create_swap_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y, true, 0, 0,
    );

    let res = send(&mut svm, &[swap_ix], &payer, &[&payer]);
    assert!(res.is_err(), "Zero-amount swaps must be rejected");
}

#[test]
fn test_slippage_protection() {
    let (mut svm, payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y) = setup();

    let init_ix = create_initialise_ix(
        &mut svm, &payer, mint_x, mint_y, config, mint_lp, vault_x, vault_y, 30,
    );
    let deposit_ix = create_deposit_ix(
        &mut svm, &payer, mint_x, mint_y, mint_lp, config, vault_x, vault_y,
    );

    assert!(send(&mut svm, &[init_ix, deposit_ix], &payer, &[&payer]).is_ok());

    let swap_ix = create_swap_ix(
        &mut svm,
        &payer,
        mint_x,
        mint_y,
        mint_lp,
        config,
        vault_x,
        vault_y,
        true,
        10_000_000,
        u64::MAX,
    );

    let res = send(&mut svm, &[swap_ix], &payer, &[&payer]);
    assert!(
        res.is_err(),
        "Swap must fail when minimum output is impossible"
    );
}
