use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

const SYSTEM_PROGRAM: Pubkey = Pubkey::new_from_array([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);

/// `create_dvp` — args: amount_a, amount_b, expiry_timestamp, nonce, ref_string, user_a_settlement_destination, user_b_settlement_destination, earliest_settlement_timestamp
pub fn build_create_dvp_ix(
    program_id: Pubkey,
    payer: Pubkey,
    swap_dvp: Pubkey,
    nonce_tombstone: Pubkey,
    settlement_authority: Pubkey,
    user_a: Pubkey,
    user_b: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    dvp_ata_a: Pubkey,
    dvp_ata_b: Pubkey,
    token_program_a: Pubkey,
    token_program_b: Pubkey,
    associated_token_program: Pubkey,
    amount_a: u64,
    amount_b: u64,
    expiry_timestamp: i64,
    nonce: u64,
    ref_string: Option<String>,
    user_a_settlement_destination: Option<[u8; 32]>,
    user_b_settlement_destination: Option<[u8; 32]>,
    earliest_settlement_timestamp: Option<i64>,
) -> Instruction {
    let mut data: Vec<u8> = vec![0];
    data.extend_from_slice(&amount_a.to_le_bytes());
    data.extend_from_slice(&amount_b.to_le_bytes());
    data.extend_from_slice(&expiry_timestamp.to_le_bytes());
    data.extend_from_slice(&nonce.to_le_bytes());
    data.extend_from_slice(&borsh::to_vec(&ref_string).unwrap());
    data.extend_from_slice(&borsh::to_vec(&user_a_settlement_destination).unwrap());
    data.extend_from_slice(&borsh::to_vec(&user_b_settlement_destination).unwrap());
    data.extend_from_slice(&borsh::to_vec(&earliest_settlement_timestamp).unwrap());
    let accounts = vec![
        AccountMeta::new(payer, true),
        AccountMeta::new(swap_dvp, false),
        AccountMeta::new(nonce_tombstone, false),
        AccountMeta::new_readonly(settlement_authority, false),
        AccountMeta::new_readonly(user_a, false),
        AccountMeta::new_readonly(user_b, false),
        AccountMeta::new_readonly(mint_a, false),
        AccountMeta::new_readonly(mint_b, false),
        AccountMeta::new(dvp_ata_a, false),
        AccountMeta::new(dvp_ata_b, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM, false),
        AccountMeta::new_readonly(token_program_a, false),
        AccountMeta::new_readonly(token_program_b, false),
        AccountMeta::new_readonly(associated_token_program, false),
    ];
    Instruction {
        program_id,
        accounts,
        data,
    }
}

/// `reclaim_dvp` — no arguments
pub fn build_reclaim_dvp_ix(
    program_id: Pubkey,
    signer: Pubkey,
    swap_dvp: Pubkey,
    mint: Pubkey,
    dvp_source_ata: Pubkey,
    signer_dest_ata: Pubkey,
    token_program: Pubkey,
    memo_program: Pubkey,
) -> Instruction {
    let mut data: Vec<u8> = vec![1];
    let accounts = vec![
        AccountMeta::new_readonly(signer, true),
        AccountMeta::new_readonly(swap_dvp, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(dvp_source_ata, false),
        AccountMeta::new(signer_dest_ata, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(memo_program, false),
    ];
    Instruction {
        program_id,
        accounts,
        data,
    }
}

/// `settle_dvp` — args: leg_a_extras_count
pub fn build_settle_dvp_ix(
    program_id: Pubkey,
    settlement_authority: Pubkey,
    swap_dvp: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    dvp_ata_a: Pubkey,
    dvp_ata_b: Pubkey,
    user_a_destination_ata_b: Pubkey,
    user_b_destination_ata_a: Pubkey,
    user_a_ata_a: Pubkey,
    user_b_ata_b: Pubkey,
    token_program_a: Pubkey,
    token_program_b: Pubkey,
    memo_program: Pubkey,
    leg_a_extras_count: u8,
) -> Instruction {
    let mut data: Vec<u8> = vec![2];
    data.extend_from_slice(&leg_a_extras_count.to_le_bytes());
    let accounts = vec![
        AccountMeta::new(settlement_authority, true),
        AccountMeta::new(swap_dvp, false),
        AccountMeta::new_readonly(mint_a, false),
        AccountMeta::new_readonly(mint_b, false),
        AccountMeta::new(dvp_ata_a, false),
        AccountMeta::new(dvp_ata_b, false),
        AccountMeta::new(user_a_destination_ata_b, false),
        AccountMeta::new(user_b_destination_ata_a, false),
        AccountMeta::new(user_a_ata_a, false),
        AccountMeta::new(user_b_ata_b, false),
        AccountMeta::new_readonly(token_program_a, false),
        AccountMeta::new_readonly(token_program_b, false),
        AccountMeta::new_readonly(memo_program, false),
    ];
    Instruction {
        program_id,
        accounts,
        data,
    }
}

/// `cancel_dvp` — args: leg_a_extras_count
pub fn build_cancel_dvp_ix(
    program_id: Pubkey,
    settlement_authority: Pubkey,
    swap_dvp: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    dvp_ata_a: Pubkey,
    dvp_ata_b: Pubkey,
    user_a_ata_a: Pubkey,
    user_b_ata_b: Pubkey,
    token_program_a: Pubkey,
    token_program_b: Pubkey,
    memo_program: Pubkey,
    leg_a_extras_count: u8,
) -> Instruction {
    let mut data: Vec<u8> = vec![3];
    data.extend_from_slice(&leg_a_extras_count.to_le_bytes());
    let accounts = vec![
        AccountMeta::new(settlement_authority, true),
        AccountMeta::new(swap_dvp, false),
        AccountMeta::new_readonly(mint_a, false),
        AccountMeta::new_readonly(mint_b, false),
        AccountMeta::new(dvp_ata_a, false),
        AccountMeta::new(dvp_ata_b, false),
        AccountMeta::new(user_a_ata_a, false),
        AccountMeta::new(user_b_ata_b, false),
        AccountMeta::new_readonly(token_program_a, false),
        AccountMeta::new_readonly(token_program_b, false),
        AccountMeta::new_readonly(memo_program, false),
    ];
    Instruction {
        program_id,
        accounts,
        data,
    }
}

/// `reject_dvp` — args: leg_a_extras_count
pub fn build_reject_dvp_ix(
    program_id: Pubkey,
    signer: Pubkey,
    swap_dvp: Pubkey,
    mint_a: Pubkey,
    mint_b: Pubkey,
    dvp_ata_a: Pubkey,
    dvp_ata_b: Pubkey,
    user_a_ata_a: Pubkey,
    user_b_ata_b: Pubkey,
    token_program_a: Pubkey,
    token_program_b: Pubkey,
    memo_program: Pubkey,
    leg_a_extras_count: u8,
) -> Instruction {
    let mut data: Vec<u8> = vec![4];
    data.extend_from_slice(&leg_a_extras_count.to_le_bytes());
    let accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new(swap_dvp, false),
        AccountMeta::new_readonly(mint_a, false),
        AccountMeta::new_readonly(mint_b, false),
        AccountMeta::new(dvp_ata_a, false),
        AccountMeta::new(dvp_ata_b, false),
        AccountMeta::new(user_a_ata_a, false),
        AccountMeta::new(user_b_ata_b, false),
        AccountMeta::new_readonly(token_program_a, false),
        AccountMeta::new_readonly(token_program_b, false),
        AccountMeta::new_readonly(memo_program, false),
    ];
    Instruction {
        program_id,
        accounts,
        data,
    }
}

/// `recover_dvp` — args: settlement_authority, user_a, user_b, mint_a, mint_b, nonce
pub fn build_recover_dvp_ix(
    program_id: Pubkey,
    signer: Pubkey,
    swap_dvp: Pubkey,
    nonce_tombstone: Pubkey,
    mint: Pubkey,
    dvp_escrow_ata: Pubkey,
    signer_dest_ata: Pubkey,
    token_program: Pubkey,
    memo_program: Pubkey,
    settlement_authority: [u8; 32],
    user_a: [u8; 32],
    user_b: [u8; 32],
    mint_a: [u8; 32],
    mint_b: [u8; 32],
    nonce: u64,
) -> Instruction {
    let mut data: Vec<u8> = vec![5];
    data.extend_from_slice(settlement_authority.as_ref());
    data.extend_from_slice(user_a.as_ref());
    data.extend_from_slice(user_b.as_ref());
    data.extend_from_slice(mint_a.as_ref());
    data.extend_from_slice(mint_b.as_ref());
    data.extend_from_slice(&nonce.to_le_bytes());
    let accounts = vec![
        AccountMeta::new(signer, true),
        AccountMeta::new_readonly(swap_dvp, false),
        AccountMeta::new_readonly(nonce_tombstone, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(dvp_escrow_ata, false),
        AccountMeta::new(signer_dest_ata, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(memo_program, false),
    ];
    Instruction {
        program_id,
        accounts,
        data,
    }
}
