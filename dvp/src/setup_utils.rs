use crate::fuzz_utils::{Action, DvpRecord};
use crate::ix_builder::{
    build_cancel_dvp_ix, build_create_dvp_ix, build_reclaim_dvp_ix, build_recover_dvp_ix,
    build_reject_dvp_ix, build_settle_dvp_ix,
};
use crate::DvpSwapProgramFixture;
use crucible_fuzzer::anchor_spl::token;
use crucible_fuzzer::*;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use spl_token_2022::extension::BaseStateWithExtensionsMut;
use spl_token_2022::{
    extension::{
        interest_bearing_mint::InterestBearingConfig, non_transferable::NonTransferable,
        transfer_fee::TransferFeeConfig, ExtensionType, StateWithExtensionsMut,
    },
    state::Mint as T22Mint,
};
use std::sync::Arc;

// ── Setup helpers ──────────────────────────────────────────────────────────────
// Each function owns one DVP shape. Returns Some(DvpRecord) on success, None if
// the on-chain create failed. setup() calls them and decides where to push.
//
// All token operations use litesvm_token builders (MintTo, TransferChecked,
// CreateAssociatedTokenAccountIdempotent). Passing token_program selects
// SPL vs Token-2022 — no separate per-program functions needed.

// ── ATA creation ──────────────────────────────────────────────────────────────
// Unified for both SPL and T22 via CreateAssociatedTokenAccountIdempotent.
// The token_program parameter selects which namespace to create under.

fn create_ata_for_program(
    ctx: &mut TestContext,
    actors: &[Arc<Keypair>],
    owner: Pubkey,
    mint: Pubkey,
    token_program: Pubkey,
) {
    let r = litesvm_token::CreateAssociatedTokenAccountIdempotent::new(
        &mut ctx.svm,
        &*actors[0],
        &mint,
    )
    .owner(&owner)
    .token_program_id(&token_program)
    .send();

    if let Err(e) = r {
        eprintln!(
            "[create_ata] failed — owner: {:?}, mint: {:?}, token_program: {:?}, err: {:?}",
            owner, mint, token_program, e
        );
    }
}

// ── DVP shape: fully funded, all four party ATAs ───────────────────────────────
// Covers SPL exact-funded (fund_extra=0), SPL surplus (fund_extra=1), and T22.
// `token_program` selects which program handles minting and transfers.

pub fn setup_dvp_funded(
    ctx: &mut TestContext,
    program_id: Pubkey,
    actors: &[Arc<Keypair>],
    mint_a: Pubkey,
    mint_b: Pubkey,
    nonce: u64,
    amount_a: u64,
    amount_b: u64,
    fund_extra: u64,
    token_program: Pubkey,
    label: &str,
) -> Option<DvpRecord> {
    let authority = actors[0].pubkey();
    let user_a = actors[1].pubkey();
    let user_b = actors[2].pubkey();

    let swap_dvp =
        derive_swap_dvp_with_mints(program_id, authority, user_a, user_b, mint_a, mint_b, nonce);
    let tombstone = derive_tombstone(program_id, swap_dvp);
    let dvp_ata_a = derive_ata(swap_dvp, mint_a, token_program);
    let dvp_ata_b = derive_ata(swap_dvp, mint_b, token_program);

    let ix = build_create_dvp_ix(
        program_id,
        actors[0].pubkey(),
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        token_program,
        token_program,
        DvpSwapProgramFixture::ATA_PROGRAM,
        amount_a,
        amount_b,
        365i64 * 24 * 60 * 60,
        nonce,
        None,
        None,
        None,
        None,
    );
    let ok = ctx
        .raw_call(ix)
        .signers(&[&*actors[0]])
        .send()
        .map(|o| o.is_success())
        .unwrap_or(false);
    assert!(ok, "[SETUP] {} create failed", label);

    // All four party ATAs — settle needs ua_b and ub_a as destination ATAs.
    let ua_a = derive_ata(user_a, mint_a, token_program);
    let ua_b = derive_ata(user_a, mint_b, token_program);
    let ub_a = derive_ata(user_b, mint_a, token_program);
    let ub_b = derive_ata(user_b, mint_b, token_program);
    for (owner, mint) in [
        (user_a, mint_a),
        (user_a, mint_b),
        (user_b, mint_a),
        (user_b, mint_b),
    ] {
        create_ata_for_program(ctx, actors, owner, mint, token_program);
    }

    // MintTo source ATAs.
    for (mint, dest, amount) in [
        (mint_a, ua_a, amount_a + fund_extra),
        (mint_b, ub_b, amount_b + fund_extra),
    ] {
        litesvm_token::MintTo::new(&mut ctx.svm, &*actors[0], &mint, &dest, amount)
            .owner(&actors[0])
            .token_program_id(&token_program)
            .send()
            .unwrap_or_else(|e| eprintln!("[SETUP {}] mint_to {:?} failed: {:?}", label, dest, e));
    }

    // TransferChecked into escrows — consistent with what the program itself uses
    // for settle, refund, recover, and reclaim.
    for (src, dst, signer_idx, mint, amount) in [
        (ua_a, dvp_ata_a, 1usize, mint_a, amount_a + fund_extra),
        (ub_b, dvp_ata_b, 2usize, mint_b, amount_b + fund_extra),
    ] {
        litesvm_token::TransferChecked::new(
            &mut ctx.svm,
            &*actors[signer_idx],
            &mint,
            &dst,
            amount,
        )
        .source(&src)
        .owner(&actors[signer_idx])
        .token_program_id(&token_program)
        .send()
        .unwrap_or_else(|e| {
            eprintln!(
                "[SETUP {}] transfer {:?}->{:?} failed: {:?}",
                label, src, dst, e
            )
        });
    }

    eprintln!(
        "[SETUP {}] dvp_ata_a: {:?}",
        label,
        ctx.svm
            .get_account(&dvp_ata_a)
            .map(|a| u64::from_le_bytes(a.data[64..72].try_into().unwrap_or([0; 8])))
    );
    eprintln!(
        "[SETUP {}] dvp_ata_b: {:?}",
        label,
        ctx.svm
            .get_account(&dvp_ata_b)
            .map(|a| u64::from_le_bytes(a.data[64..72].try_into().unwrap_or([0; 8])))
    );

    Some(DvpRecord {
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        nonce,
        amount_a,
        amount_b,
        token_program_a: token_program,
        token_program_b: token_program,
    })
}

// ── DVP shape: created then rejected → closed ─────────────────────────────────
// Reject is SPL-only in the current program. Synthetic escrow accounts are
// injected post-reject so recover actions have something to drain.

pub fn setup_dvp_closed(
    ctx: &mut TestContext,
    program_id: Pubkey,
    actors: &[Arc<Keypair>],
    mint_a: Pubkey,
    mint_b: Pubkey,
    nonce: u64,
    amount_a: u64,
    amount_b: u64,
    preload: u64,
    label: &str,
) -> Option<DvpRecord> {
    let authority = actors[0].pubkey();
    let user_a = actors[1].pubkey();
    let user_b = actors[2].pubkey();

    let swap_dvp =
        derive_swap_dvp_with_mints(program_id, authority, user_a, user_b, mint_a, mint_b, nonce);
    let tombstone = derive_tombstone(program_id, swap_dvp);
    let dvp_ata_a = derive_ata(swap_dvp, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let dvp_ata_b = derive_ata(swap_dvp, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM);

    let ix = build_create_dvp_ix(
        program_id,
        actors[0].pubkey(),
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        DvpSwapProgramFixture::TOKEN_PROGRAM,
        DvpSwapProgramFixture::TOKEN_PROGRAM,
        DvpSwapProgramFixture::ATA_PROGRAM,
        amount_a,
        amount_b,
        365i64 * 24 * 60 * 60,
        nonce,
        None,
        None,
        None,
        None,
    );
    let ok = ctx
        .raw_call(ix)
        .signers(&[&*actors[0]])
        .send()
        .map(|o| o.is_success())
        .unwrap_or(false);
    assert!(ok, "[SETUP] {} create failed", label);

    // Reject needs the two return ATAs to exist (unfunded is fine).
    let ua_a = derive_ata(user_a, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let ub_b = derive_ata(user_b, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM);
    for (owner, mint) in [(user_a, mint_a), (user_b, mint_b)] {
        create_ata_for_program(
            ctx,
            actors,
            owner,
            mint,
            DvpSwapProgramFixture::TOKEN_PROGRAM,
        );
    }

    let reject_ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(user_a, true),
            AccountMeta::new(swap_dvp, false),
            AccountMeta::new_readonly(mint_a, false),
            AccountMeta::new_readonly(mint_b, false),
            AccountMeta::new(dvp_ata_a, false),
            AccountMeta::new(dvp_ata_b, false),
            AccountMeta::new(ua_a, false),
            AccountMeta::new(ub_b, false),
            AccountMeta::new_readonly(DvpSwapProgramFixture::TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(DvpSwapProgramFixture::TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(DvpSwapProgramFixture::MEMO_PROGRAM, false),
        ],
        data: vec![4u8, 0u8], // discriminator=4, leg_a_extras_count=0
    };
    let r = ctx.raw_call(reject_ix).signers(&[&*actors[1]]).send();
    eprintln!(
        "[SETUP {}] reject: {:?}",
        label,
        r.as_ref().map(|o| o.is_success())
    );

    // Synthetic escrow accounts so recover actions have something to drain.
    // Intentional ctx.create_token_account() — injecting a preloaded balance
    // that CreateAssociatedTokenAccountIdempotent cannot do.
    for (mint, escrow) in [(mint_a, dvp_ata_a), (mint_b, dvp_ata_b)] {
        ctx.create_token_account()
            .pubkey(escrow)
            .mint(mint)
            .token_owner(swap_dvp)
            .amount(preload)
            .create()
            .unwrap();
    }

    Some(DvpRecord {
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        nonce,
        amount_a,
        amount_b,
        token_program_a: DvpSwapProgramFixture::TOKEN_PROGRAM,
        token_program_b: DvpSwapProgramFixture::TOKEN_PROGRAM,
    })
}

// ── DVP shape: short-expiry, not funded ───────────────────────────────────────
// expiry=1s from epoch 0 — any slot warp exceeds it, enabling the reclaim path.
// Not funded: the point is expiry, not settle.
// Returns None if create failed (clock already past expiry).

pub fn setup_dvp_short_expiry(
    ctx: &mut TestContext,
    program_id: Pubkey,
    actors: &[Arc<Keypair>],
    mint_a: Pubkey,
    mint_b: Pubkey,
    nonce: u64,
    amount_a: u64,
    amount_b: u64,
    label: &str,
) -> Option<DvpRecord> {
    let authority = actors[0].pubkey();
    let user_a = actors[1].pubkey();
    let user_b = actors[2].pubkey();

    let swap_dvp =
        derive_swap_dvp_with_mints(program_id, authority, user_a, user_b, mint_a, mint_b, nonce);
    let tombstone = derive_tombstone(program_id, swap_dvp);
    let dvp_ata_a = derive_ata(swap_dvp, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let dvp_ata_b = derive_ata(swap_dvp, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM);

    let ix = build_create_dvp_ix(
        program_id,
        actors[0].pubkey(),
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        DvpSwapProgramFixture::TOKEN_PROGRAM,
        DvpSwapProgramFixture::TOKEN_PROGRAM,
        DvpSwapProgramFixture::ATA_PROGRAM,
        amount_a,
        amount_b,
        1i64, // expires 1s from epoch 0
        nonce,
        None,
        None,
        None,
        None,
    );
    let ok = ctx
        .raw_call(ix)
        .signers(&[&*actors[0]])
        .send()
        .map(|o| o.is_success())
        .unwrap_or(false);
    eprintln!("[SETUP {}] create short-expiry: {}", label, ok);
    if !ok {
        return None;
    }

    Some(DvpRecord {
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        nonce,
        amount_a,
        amount_b,
        token_program_a: DvpSwapProgramFixture::TOKEN_PROGRAM,
        token_program_b: DvpSwapProgramFixture::TOKEN_PROGRAM,
    })
}

// ── DVP shape: earliest settlement constraint, funded ─────────────────────────
// Settle is blocked until clock >= earliest. Funded so that once the fuzzer
// warps the clock past earliest, the settle path is immediately reachable.

pub fn setup_dvp_earliest(
    ctx: &mut TestContext,
    program_id: Pubkey,
    actors: &[Arc<Keypair>],
    mint_a: Pubkey,
    mint_b: Pubkey,
    nonce: u64,
    amount_a: u64,
    amount_b: u64,
    earliest: i64,
    token_program: Pubkey,
    label: &str,
) -> Option<DvpRecord> {
    let authority = actors[0].pubkey();
    let user_a = actors[1].pubkey();
    let user_b = actors[2].pubkey();

    let swap_dvp =
        derive_swap_dvp_with_mints(program_id, authority, user_a, user_b, mint_a, mint_b, nonce);
    let tombstone = derive_tombstone(program_id, swap_dvp);
    let dvp_ata_a = derive_ata(swap_dvp, mint_a, token_program);
    let dvp_ata_b = derive_ata(swap_dvp, mint_b, token_program);

    let ix = build_create_dvp_ix(
        program_id,
        actors[0].pubkey(),
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        token_program,
        token_program,
        DvpSwapProgramFixture::ATA_PROGRAM,
        amount_a,
        amount_b,
        365i64 * 24 * 60 * 60,
        nonce,
        None,
        None,
        None,
        Some(earliest),
    );
    let ok = ctx
        .raw_call(ix)
        .signers(&[&*actors[0]])
        .send()
        .map(|o| o.is_success())
        .unwrap_or(false);
    eprintln!("[SETUP {}] create earliest: {}", label, ok);
    if !ok {
        return None;
    }

    // All four party ATAs — needed once clock is warped past earliest.
    let ua_a = derive_ata(user_a, mint_a, token_program);
    let ua_b = derive_ata(user_a, mint_b, token_program);
    let ub_a = derive_ata(user_b, mint_a, token_program);
    let ub_b = derive_ata(user_b, mint_b, token_program);
    for (owner, mint) in [
        (user_a, mint_a),
        (user_a, mint_b),
        (user_b, mint_a),
        (user_b, mint_b),
    ] {
        create_ata_for_program(ctx, actors, owner, mint, token_program);
    }

    for (mint, dest, amount) in [(mint_a, ua_a, amount_a), (mint_b, ub_b, amount_b)] {
        litesvm_token::MintTo::new(&mut ctx.svm, &*actors[0], &mint, &dest, amount)
            .owner(&actors[0])
            .token_program_id(&token_program)
            .send()
            .unwrap_or_else(|e| eprintln!("[SETUP {}] mint_to {:?} failed: {:?}", label, dest, e));
    }

    for (src, dst, signer_idx, mint, amount) in [
        (ua_a, dvp_ata_a, 1usize, mint_a, amount_a),
        (ub_b, dvp_ata_b, 2usize, mint_b, amount_b),
    ] {
        litesvm_token::TransferChecked::new(
            &mut ctx.svm,
            &*actors[signer_idx],
            &mint,
            &dst,
            amount,
        )
        .source(&src)
        .owner(&actors[signer_idx])
        .token_program_id(&token_program)
        .send()
        .unwrap_or_else(|e| {
            eprintln!(
                "[SETUP {}] transfer {:?}->{:?} failed: {:?}",
                label, src, dst, e
            )
        });
    }

    Some(DvpRecord {
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        nonce,
        amount_a,
        amount_b,
        token_program_a: token_program,
        token_program_b: token_program,
    })
}

// ── DVP shape: created but not yet funded ─────────────────────────────────────
// Only the two source ATAs are created. The fuzzer funds via action_fund_legs.
// Supports both SPL and T22 via token_program.

pub fn setup_dvp_unfunded(
    ctx: &mut TestContext,
    program_id: Pubkey,
    actors: &[Arc<Keypair>],
    mint_a: Pubkey,
    mint_b: Pubkey,
    nonce: u64,
    amount_a: u64,
    amount_b: u64,
    token_program: Pubkey,
    label: &str,
) -> Option<DvpRecord> {
    let authority = actors[0].pubkey();
    let user_a = actors[1].pubkey();
    let user_b = actors[2].pubkey();

    let swap_dvp =
        derive_swap_dvp_with_mints(program_id, authority, user_a, user_b, mint_a, mint_b, nonce);
    let tombstone = derive_tombstone(program_id, swap_dvp);
    let dvp_ata_a = derive_ata(swap_dvp, mint_a, token_program);
    let dvp_ata_b = derive_ata(swap_dvp, mint_b, token_program);

    let ix = build_create_dvp_ix(
        program_id,
        actors[0].pubkey(),
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        token_program,
        token_program,
        DvpSwapProgramFixture::ATA_PROGRAM,
        amount_a,
        amount_b,
        365i64 * 24 * 60 * 60,
        nonce,
        None,
        None,
        None,
        None,
    );
    let ok = ctx
        .raw_call(ix)
        .signers(&[&*actors[0]])
        .send()
        .map(|o| o.is_success())
        .unwrap_or(false);
    eprintln!("[SETUP {}] create unfunded: {}", label, ok);
    if !ok {
        return None;
    }

    // Only the two source ATAs — action_fund_legs creates the destination ATAs.
    for (owner, mint) in [(user_a, mint_a), (user_b, mint_b)] {
        create_ata_for_program(ctx, actors, owner, mint, token_program);
    }

    Some(DvpRecord {
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        nonce,
        amount_a,
        amount_b,
        token_program_a: token_program,
        token_program_b: token_program,
    })
}

// ── DVP shape: leg A funded, leg B empty ──────────────────────────────────────
// Covers: reclaim no-op on empty leg, reclaim transfer on funded leg,
// reject/cancel with one unfunded leg hitting the skip-transfer branch.

pub fn setup_dvp_partial_funded(
    ctx: &mut TestContext,
    program_id: Pubkey,
    actors: &[Arc<Keypair>],
    mint_a: Pubkey,
    mint_b: Pubkey,
    nonce: u64,
    amount_a: u64,
    amount_b: u64,
    label: &str,
) -> Option<DvpRecord> {
    let authority = actors[0].pubkey();
    let user_a = actors[1].pubkey();
    let user_b = actors[2].pubkey();

    let swap_dvp =
        derive_swap_dvp_with_mints(program_id, authority, user_a, user_b, mint_a, mint_b, nonce);
    let tombstone = derive_tombstone(program_id, swap_dvp);
    let dvp_ata_a = derive_ata(swap_dvp, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let dvp_ata_b = derive_ata(swap_dvp, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM);

    let ix = build_create_dvp_ix(
        program_id,
        actors[0].pubkey(),
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        DvpSwapProgramFixture::TOKEN_PROGRAM,
        DvpSwapProgramFixture::TOKEN_PROGRAM,
        DvpSwapProgramFixture::ATA_PROGRAM,
        amount_a,
        amount_b,
        365i64 * 24 * 60 * 60,
        nonce,
        None,
        None,
        None,
        None,
    );
    let ok = ctx
        .raw_call(ix)
        .signers(&[&*actors[0]])
        .send()
        .map(|o| o.is_success())
        .unwrap_or(false);
    assert!(ok, "[SETUP] {} create failed", label);

    // All four ATAs — reject/cancel need ua_b and ub_a even when empty.
    let ua_a = derive_ata(user_a, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let ua_b = derive_ata(user_a, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let ub_a = derive_ata(user_b, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let ub_b = derive_ata(user_b, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM);
    for (owner, mint) in [
        (user_a, mint_a),
        (user_a, mint_b),
        (user_b, mint_a),
        (user_b, mint_b),
    ] {
        create_ata_for_program(
            ctx,
            actors,
            owner,
            mint,
            DvpSwapProgramFixture::TOKEN_PROGRAM,
        );
    }

    // Fund only leg A.
    litesvm_token::MintTo::new(&mut ctx.svm, &*actors[0], &mint_a, &ua_a, amount_a)
        .owner(&actors[0])
        .token_program_id(&DvpSwapProgramFixture::TOKEN_PROGRAM)
        .send()
        .unwrap_or_else(|e| eprintln!("[SETUP {}] mint_to failed: {:?}", label, e));

    litesvm_token::TransferChecked::new(&mut ctx.svm, &*actors[1], &mint_a, &dvp_ata_a, amount_a)
        .source(&ua_a)
        .owner(&actors[1])
        .token_program_id(&DvpSwapProgramFixture::TOKEN_PROGRAM)
        .send()
        .unwrap_or_else(|e| eprintln!("[SETUP {}] transfer leg A failed: {:?}", label, e));

    // Leg B escrow exists (created by CreateDvp CPI) but holds zero tokens.

    Some(DvpRecord {
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        nonce,
        amount_a,
        amount_b,
        token_program_a: DvpSwapProgramFixture::TOKEN_PROGRAM,
        token_program_b: DvpSwapProgramFixture::TOKEN_PROGRAM,
    })
}

// ── DVP shape: mixed token programs (SPL leg A, T22 leg B) ────────────────────
// The only setup DVP that exercises token_program_a != token_program_b,
// which Settle, Reclaim, and Reject each handle with per-leg dispatch.

pub fn setup_dvp_mixed(
    ctx: &mut TestContext,
    program_id: Pubkey,
    actors: &[Arc<Keypair>],
    mint_a: Pubkey, // SPL mint
    mint_b: Pubkey, // T22 mint
    nonce: u64,
    amount_a: u64,
    amount_b: u64,
    label: &str,
) -> Option<DvpRecord> {
    let authority = actors[0].pubkey();
    let user_a = actors[1].pubkey();
    let user_b = actors[2].pubkey();

    let swap_dvp =
        derive_swap_dvp_with_mints(program_id, authority, user_a, user_b, mint_a, mint_b, nonce);
    let tombstone = derive_tombstone(program_id, swap_dvp);
    let dvp_ata_a = derive_ata(swap_dvp, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let dvp_ata_b = derive_ata(swap_dvp, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM_2022);

    let ix = build_create_dvp_ix(
        program_id,
        actors[0].pubkey(),
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        DvpSwapProgramFixture::TOKEN_PROGRAM,
        DvpSwapProgramFixture::TOKEN_PROGRAM_2022,
        DvpSwapProgramFixture::ATA_PROGRAM,
        amount_a,
        amount_b,
        365i64 * 24 * 60 * 60,
        nonce,
        None,
        None,
        None,
        None,
    );
    let ok = ctx
        .raw_call(ix)
        .signers(&[&*actors[0]])
        .send()
        .map(|o| o.is_success())
        .unwrap_or(false);
    eprintln!("[SETUP {}] create mixed: {}", label, ok);
    if !ok {
        return None;
    }

    // SPL ATAs for leg A.
    let ua_a = derive_ata(user_a, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    let ub_a = derive_ata(user_b, mint_a, DvpSwapProgramFixture::TOKEN_PROGRAM);
    for owner in [user_a, user_b] {
        create_ata_for_program(
            ctx,
            actors,
            owner,
            mint_a,
            DvpSwapProgramFixture::TOKEN_PROGRAM,
        );
    }

    // T22 ATAs for leg B.
    let ua_b = derive_ata(user_a, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM_2022);
    let ub_b = derive_ata(user_b, mint_b, DvpSwapProgramFixture::TOKEN_PROGRAM_2022);
    for owner in [user_a, user_b] {
        create_ata_for_program(
            ctx,
            actors,
            owner,
            mint_b,
            DvpSwapProgramFixture::TOKEN_PROGRAM_2022,
        );
    }

    // Fund leg A via SPL.
    litesvm_token::MintTo::new(&mut ctx.svm, &*actors[0], &mint_a, &ua_a, amount_a)
        .owner(&actors[0])
        .token_program_id(&DvpSwapProgramFixture::TOKEN_PROGRAM)
        .send()
        .unwrap_or_else(|e| eprintln!("[SETUP {}] mint_to leg A failed: {:?}", label, e));

    litesvm_token::TransferChecked::new(&mut ctx.svm, &*actors[1], &mint_a, &dvp_ata_a, amount_a)
        .source(&ua_a)
        .owner(&actors[1])
        .token_program_id(&DvpSwapProgramFixture::TOKEN_PROGRAM)
        .send()
        .unwrap_or_else(|e| eprintln!("[SETUP {}] transfer leg A failed: {:?}", label, e));

    // Fund leg B via T22.
    litesvm_token::MintTo::new(&mut ctx.svm, &*actors[0], &mint_b, &ub_b, amount_b)
        .owner(&actors[0])
        .token_program_id(&DvpSwapProgramFixture::TOKEN_PROGRAM_2022)
        .send()
        .unwrap_or_else(|e| eprintln!("[SETUP {}] mint_to leg B failed: {:?}", label, e));

    litesvm_token::TransferChecked::new(&mut ctx.svm, &*actors[2], &mint_b, &dvp_ata_b, amount_b)
        .source(&ub_b)
        .owner(&actors[2])
        .token_program_id(&DvpSwapProgramFixture::TOKEN_PROGRAM_2022)
        .send()
        .unwrap_or_else(|e| eprintln!("[SETUP {}] transfer leg B failed: {:?}", label, e));

    Some(DvpRecord {
        swap_dvp,
        tombstone,
        authority,
        user_a,
        user_b,
        mint_a,
        mint_b,
        dvp_ata_a,
        dvp_ata_b,
        nonce,
        amount_a,
        amount_b,
        token_program_a: DvpSwapProgramFixture::TOKEN_PROGRAM,
        token_program_b: DvpSwapProgramFixture::TOKEN_PROGRAM_2022,
    })
}

// ── PDA derivation helpers ─────────────────────────────────────────────────────

fn derive_swap_dvp_with_mints(
    program_id: Pubkey,
    authority: Pubkey,
    user_a: Pubkey,
    user_b: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    nonce: u64,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"dvp",
            authority.as_ref(),
            user_a.as_ref(),
            user_b.as_ref(),
            mint_a.as_ref(),
            mint_b.as_ref(),
            &nonce.to_le_bytes(),
        ],
        &program_id,
    )
    .0
}

fn derive_tombstone(program_id: Pubkey, swap_dvp: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"nonce", swap_dvp.as_ref()], &program_id).0
}

fn derive_ata(wallet: Pubkey, mint: Pubkey, token_program: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
        &DvpSwapProgramFixture::ATA_PROGRAM,
    )
    .0
}
