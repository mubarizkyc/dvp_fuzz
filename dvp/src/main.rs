// Generated from the program IDL — do not edit by hand.
// program: dvp_swap_program   address: DzG1qJupt6Khm8s8jB3p93NkhPoiAg2M7vkEhkS15CtC
// 6 instructions, 3 actor(s), 0 PDA(s)
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
mod fuzz_utils;
mod ix_builder;
mod setup_utils;
use fuzz_utils::{note_blocked, note_landed, Action, DvpRecord};
use ix_builder::{
    build_cancel_dvp_ix, build_create_dvp_ix, build_reclaim_dvp_ix, build_recover_dvp_ix,
    build_reject_dvp_ix, build_settle_dvp_ix,
};
use setup_utils::{
    setup_dvp_closed, setup_dvp_earliest, setup_dvp_funded, setup_dvp_mixed,
    setup_dvp_partial_funded, setup_dvp_short_expiry, setup_dvp_unfunded,
};
const SYSTEM_PROGRAM: Pubkey = Pubkey::new_from_array([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
]);
const PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    192, 247, 83, 118, 97, 24, 213, 133, 240, 59, 120, 203, 107, 83, 244, 165, 210, 47, 254, 235,
    206, 164, 255, 136, 252, 174, 75, 17, 249, 220, 129, 37,
]);
const FUNDED: u64 = 100000000000; // what create_actor gives each actor
const MAX_ACTORS: usize = 8; // a bound, not a choice: the
const WATCHED_MAX: usize = 48; // accounts snapshot() carries...

#[derive(Clone)]
struct DvpSwapProgramFixture {
    pub ctx: TestContext,
    program_id: Pubkey,
    // state before the current action; filled by snapshot(), read as old(x)
    prev_lamports: std::collections::HashMap<Pubkey, u64>,
    prev_data: std::collections::HashMap<Pubkey, Vec<u8>>,
    prev_slot: u64,
    // what the current action is; filled before it is sent, read by properties
    last: Action,
    // every actor action_create_actor has made, in creation order
    actors: Vec<Arc<Keypair>>,
    // accounts any action has passed to the program; snapshot() reads these
    watched: Vec<Pubkey>,
    // the two token-program-owned mints create_dvp needs
    mints: Vec<Pubkey>,
    // every DvP that actually got created, with its derived addresses
    dvps: Vec<DvpRecord>,
    // DvPs whose SwapDvp is closed but whose tombstone survives
    closed: Vec<DvpRecord>,
}
#[fuzz_fixture]
impl DvpSwapProgramFixture {
    pub fn setup() -> Self {
        let mut ctx = TestContext::new();
        let program_id = PROGRAM_ID;
        ctx.add_program(
            &program_id,
            "/home/mubariz/Documents/dvp/source/target/deploy/dvp_swap_program.so",
        )
        .unwrap();

        // ── Actors ────────────────────────────────────────────────────────────
        let mut actors: Vec<Arc<Keypair>> = Vec::new();
        for i in 0..MAX_ACTORS {
            let seed = (i as u8).wrapping_add(1);
            let keypair = Arc::new(Keypair::new_from_array([seed; 32]));
            ctx.create_account()
                .pubkey(keypair.pubkey())
                .lamports(FUNDED)
                .owner(SYSTEM_PROGRAM)
                .create()
                .unwrap();
            actors.push(keypair);
        }

        // ── SPL Token mints (indices 0, 1) ────────────────────────────────────
        let mint_authority = actors[0].pubkey();
        let mut mints: Vec<Pubkey> = Vec::new();
        for i in 0..2u8 {
            let mint = Pubkey::new_from_array([100u8 + i; 32]);
            ctx.create_mint()
                .pubkey(mint)
                .mint_authority(mint_authority)
                .decimals(0)
                .create()
                .unwrap();
            mints.push(mint);
        }
        let mint_a = mints[0];
        let mint_b = mints[1];

        // ── Token-2022 mints (indices 2, 3) ───────────────────────────────────
        let mint_a_22 = litesvm_token::CreateMint::new(&mut ctx.svm, &*actors[0])
            .authority(&actors[0].pubkey())
            .decimals(0)
            .token_program_id(&Self::TOKEN_PROGRAM_2022)
            .send()
            .unwrap();
        let mint_b_22 = litesvm_token::CreateMint::new(&mut ctx.svm, &*actors[0])
            .authority(&actors[0].pubkey())
            .decimals(0)
            .token_program_id(&Self::TOKEN_PROGRAM_2022)
            .send()
            .unwrap();
        mints.push(mint_a_22); // index 2
        mints.push(mint_b_22); // index 3

        let create_blocked_mint = |ctx: &mut TestContext,
                                   mint_pubkey: Pubkey,
                                   ext: ExtensionType|
         -> Pubkey {
            let size = ExtensionType::try_calculate_account_len::<T22Mint>(&[ext]).unwrap();
            let mut data = vec![0u8; size];
            {
                let mut state =
                    StateWithExtensionsMut::<T22Mint>::unpack_uninitialized(&mut data).unwrap();
                match ext {
                    ExtensionType::TransferFeeConfig => {
                        state.init_extension::<TransferFeeConfig>(true).unwrap();
                    }
                    ExtensionType::InterestBearingConfig => {
                        state.init_extension::<InterestBearingConfig>(true).unwrap();
                    }
                    ExtensionType::NonTransferable => {
                        state.init_extension::<NonTransferable>(true).unwrap();
                    }
                    ExtensionType::ScaledUiAmount => {
                        state.init_extension::<spl_token_2022_interface::extension::scaled_ui_amount::ScaledUiAmountConfig>(true).unwrap();
                    }
                    _ => {}
                }
                // Set authority on state.base before pack_base — not at raw offsets.
                // pack_base() writes the base Mint layout; init_account_type() writes
                // the AccountType discriminator at the correct T22 position.
                state.base.mint_authority = solana_program_option::COption::Some(mint_authority);
                state.base.decimals = 0;
                state.base.is_initialized = true;
                state.pack_base();
                state.init_account_type().unwrap();
                // Nothing after this — the two data[0..4]/data[4..36] writes
                // from the original were overwriting already-correct packed data
                // at wrong SPL Token offsets.
            }
            let account = solana_account::Account {
                lamports: 2_039_280,
                data,
                owner: Self::TOKEN_PROGRAM_2022,
                executable: false,
                rent_epoch: u64::MAX,
            };
            ctx.svm.set_account(mint_pubkey, account.into()).unwrap();
            mint_pubkey
        };

        mints.push(create_blocked_mint(
            &mut ctx,
            Pubkey::new_from_array([110u8; 32]),
            ExtensionType::TransferFeeConfig,
        )); // index 4
        mints.push(create_blocked_mint(
            &mut ctx,
            Pubkey::new_from_array([111u8; 32]),
            ExtensionType::InterestBearingConfig,
        )); // index 5
        mints.push(create_blocked_mint(
            &mut ctx,
            Pubkey::new_from_array([112u8; 32]),
            ExtensionType::NonTransferable,
        )); // index 6
        mints.push(create_blocked_mint(
            &mut ctx,
            Pubkey::new_from_array([113u8; 32]),
            ExtensionType::ScaledUiAmount,
        )); // index 7
            // ── DVPs ──────────────────────────────────────────────────────────────
        let mut dvps: Vec<DvpRecord> = Vec::new();
        let mut closed: Vec<DvpRecord> = Vec::new();
        // DVP 1  — SPL, exact-funded, all 4 ATAs: primary settle path
        // DVP 2  — SPL, exact-funded, all 4 ATAs: second settle seat (fuzzer variety)
        // DVP 3  — SPL, exact-funded, all 4 ATAs: third settle seat (fuzzer variety)
        // DVP 4  — SPL, rejected → closed: tombstone + synthetic escrow for recover path
        // DVP 5  — SPL, short-expiry (1s), unfunded: reclaim-after-expiry path
        // DVP 6  — SPL, surplus-funded (+1 per leg): settle surplus-refund branch
        // DVP 7  — SPL, earliest constraint, funded: settle blocked until clock warp
        // DVP 8  — T22, exact-funded, all 4 ATAs: T22 settle path
        // DVP 9  — T22, unfunded: T22 DVP awaiting fund_legs action
        // DVP 10 — SPL, exact-funded, all 4 ATAs: reserved for cancel (settlement_authority signer)
        // DVP 11 — SPL, leg A funded, leg B empty: partial-fund reclaim + skip-transfer branch
        // DVP 12 — Mixed (SPL leg A, T22 leg B): only DVP with token_program_a != token_program_b
        if let Some(r) = setup_dvp_funded(
            &mut ctx,
            program_id,
            &actors,
            mint_a,
            mint_b,
            0,
            1_000,
            2_000,
            0,
            Self::TOKEN_PROGRAM,
            "DVP1",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_funded(
            &mut ctx,
            program_id,
            &actors,
            mint_a,
            mint_b,
            1,
            500,
            1_000,
            0,
            Self::TOKEN_PROGRAM,
            "DVP2",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_funded(
            &mut ctx,
            program_id,
            &actors,
            mint_a,
            mint_b,
            2,
            750,
            1_500,
            0,
            Self::TOKEN_PROGRAM,
            "DVP3",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_closed(
            &mut ctx, program_id, &actors, mint_a, mint_b, 3, 100, 200, 50, "DVP4",
        ) {
            closed.push(r);
        }
        if let Some(r) = setup_dvp_short_expiry(
            &mut ctx, program_id, &actors, mint_a, mint_b, 4, 300, 600, "DVP5",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_funded(
            &mut ctx,
            program_id,
            &actors,
            mint_a,
            mint_b,
            5,
            1_000,
            2_000,
            1,
            Self::TOKEN_PROGRAM,
            "DVP6",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_earliest(
            &mut ctx,
            program_id,
            &actors,
            mint_a,
            mint_b,
            6,
            500,
            1_000,
            365i64 * 24 * 60 * 60,
            Self::TOKEN_PROGRAM,
            "DVP7",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_funded(
            &mut ctx,
            program_id,
            &actors,
            mint_a_22,
            mint_b_22,
            7,
            1_000,
            2_000,
            0,
            Self::TOKEN_PROGRAM_2022,
            "DVP8",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_unfunded(
            &mut ctx,
            program_id,
            &actors,
            mint_a_22,
            mint_b_22,
            8,
            500,
            1_000,
            Self::TOKEN_PROGRAM_2022,
            "DVP9",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_funded(
            &mut ctx,
            program_id,
            &actors,
            mint_a,
            mint_b,
            9,
            400,
            800,
            0,
            Self::TOKEN_PROGRAM,
            "DVP10",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_partial_funded(
            &mut ctx, program_id, &actors, mint_a, mint_b, 10, 600, 1_200, "DVP11",
        ) {
            dvps.push(r);
        }
        if let Some(r) = setup_dvp_mixed(
            &mut ctx, program_id, &actors, mint_a, mint_b_22, 11, 300, 600, "DVP12",
        ) {
            dvps.push(r);
        }
        Self {
            ctx,
            program_id,
            prev_lamports: Default::default(),
            prev_data: Default::default(),
            prev_slot: 0,
            last: Action::default(),
            actors,
            watched: Vec::new(),
            mints,
            dvps,
            closed,
        }
    }
    /// Every account worth remembering, as the world stands right now.
    fn tracked_keys(&self) -> Vec<Pubkey> {
        let mut keys: Vec<Pubkey> = self.actors.iter().map(|a| a.pubkey()).collect();
        keys.extend(self.watched.iter().copied());
        keys
    }
    fn snapshot(&mut self) {
        self.prev_lamports.clear();
        self.prev_data.clear();
        self.prev_slot = self.ctx.slot();
        for key in self.tracked_keys() {
            if let Ok(account) = self.ctx.get_account(&key) {
                self.prev_lamports.insert(key, account.lamports);
                self.prev_data.insert(key, account.data.clone());
            }
        }
    }

    fn token_amount(&self, key: &Pubkey) -> Option<u64> {
        let account = self.ctx.get_account(key).ok()?;
        if account.data.len() < 72 {
            return None;
        }
        Some(u64::from_le_bytes(account.data[64..72].try_into().ok()?))
    }

    /// The mint a token account holds, for a property that has to tell the two
    /// legs of a swap apart.
    fn token_mint(&self, key: &Pubkey) -> Option<Pubkey> {
        let account = self.ctx.get_account(key).ok()?;
        if account.data.len() < 32 {
            return None;
        }
        let mut raw = [0u8; 32];
        raw.copy_from_slice(&account.data[..32]);
        Some(Pubkey::new_from_array(raw))
    }
    fn actor(&self, which: u64) -> Option<Arc<Keypair>> {
        if self.actors.is_empty() {
            return None;
        }
        Some(self.actors[(which as usize) % self.actors.len()].clone())
    }
    fn actor_key(&self, which: u64) -> [u8; 32] {
        self.actor(which)
            .map(|a| a.pubkey().to_bytes())
            .unwrap_or([0u8; 32])
    }
    fn watch(&mut self, key: Pubkey) {
        if self.watched.len() < WATCHED_MAX && !self.watched.contains(&key) {
            self.watched.push(key);
        }
    }
    fn unix_now(&self) -> i64 {
        (self.ctx.slot() as i64)
    }

    const TOKEN_PROGRAM: Pubkey = Pubkey::new_from_array([
        6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172, 28, 180, 133,
        237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
    ]);
    const TOKEN_PROGRAM_2022: Pubkey = Pubkey::new_from_array([
        6, 221, 246, 225, 238, 117, 143, 222, 24, 66, 93, 188, 228, 108, 205, 218, 182, 26, 252,
        77, 131, 185, 13, 39, 254, 189, 249, 40, 216, 161, 139, 252,
    ]);
    const ATA_PROGRAM: Pubkey = Pubkey::new_from_array([
        140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131, 11, 90, 19, 153,
        218, 255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
    ]);
    const MEMO_PROGRAM: Pubkey = Pubkey::new_from_array([
        5, 74, 83, 80, 248, 93, 200, 130, 214, 20, 165, 86, 114, 120, 138, 41, 109, 223, 30, 171,
        171, 208, 166, 6, 120, 136, 73, 50, 244, 238, 246, 160,
    ]);

    /// The SwapDvp PDA, derived exactly as process_create_dvp does.
    fn derive_swap_dvp(
        program_id: &Pubkey,
        authority: &Pubkey,
        user_a: &Pubkey,
        user_b: &Pubkey,
        mint_a: &Pubkey,
        mint_b: &Pubkey,
        nonce: u64,
    ) -> (Pubkey, u8) {
        let nonce_bytes = nonce.to_le_bytes();
        Pubkey::find_program_address(
            &[
                b"dvp",
                authority.as_ref(),
                user_a.as_ref(),
                user_b.as_ref(),
                mint_a.as_ref(),
                mint_b.as_ref(),
                &nonce_bytes,
            ],
            program_id,
        )
    }

    fn derive_ata_with_program(wallet: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[wallet.as_ref(), token_program.as_ref(), mint.as_ref()],
            &Self::ATA_PROGRAM,
        )
        .0
    }

    fn set_token_amount(&mut self, key: &Pubkey, amount: u64) -> bool {
        let mut account = match self.ctx.read_account(key) {
            Ok(a) => a,
            Err(_) => return false,
        };
        if account.data.len() < 72 {
            return false;
        }
        account.data[64..72].copy_from_slice(&amount.to_le_bytes());
        self.ctx.write_account(key, account).is_ok()
    }

    /// `create_dvp`. False if the transaction was rejected.
    #[allow(non_snake_case)]
    pub fn action_PROGRAM_create_dvp(
        &mut self,
        which_payer: u64,
        which_settlement_authority: u64,
        which_swap_dvp: u64,
        which_nonce_tombstone: u64,
        which_user_a: u64,
        which_user_b: u64,
        which_mint_a: u64,
        which_mint_b: u64,
        which_dvp_ata_a: u64,
        which_dvp_ata_b: u64,
        which_token_program_a: u64,
        which_token_program_b: u64,
        which_associated_token_program: u64,
        #[range(1..100000000000)] amount_a: u64,
        #[range(1..100000000000)] amount_b: u64,
        #[range(1..31_536_000)] expiry_delta: i64,
        #[range(0..16)] nonce: u64,
        ref_string_value_choice: u64,
        ref_string_present: u64,
        user_a_settlement_destination_value_actor: u64,
        user_a_settlement_destination_present: u64,
        user_b_settlement_destination_value_actor: u64,
        user_b_settlement_destination_present: u64,
        earliest_settlement_timestamp: Option<i64>,
    ) -> bool {
        //intentional toggling of user a /b
        let user_a = match which_user_a % 6 {
            0 if !self.mints.is_empty() => self.mints[which_user_a as usize % self.mints.len()],
            _ => Pubkey::new_from_array(self.actor_key(which_user_a)),
        };
        let user_b = match which_user_b % 6 {
            0 if !self.mints.is_empty() => self.mints[which_user_b as usize % self.mints.len()],
            _ => Pubkey::new_from_array(self.actor_key(which_user_b)),
        };
        let settlement_authority_pubkey = match which_settlement_authority % 8 {
            0 => Self::TOKEN_PROGRAM, // executable → SettlementAuthorityExecutable
            1 => Self::ATA_PROGRAM,   // executable
            _ => match self.actor(which_settlement_authority) {
                Some(a) => a.pubkey(),
                None => {
                    note_blocked("create_dvp", -1, &"no settlement auth");
                    return false;
                }
            },
        };
        let payer = match self.actor(which_payer) {
            Some(actor) => actor,
            None => {
                note_blocked("create_dvp", -1, &"no payer available");
                return false;
            }
        };

        // expiry is an absolute timestamp; compute it as now + delta so it is
        // always in range [now+1, now+365d] regardless of the current slot.
        let expiry_timestamp = self.unix_now().saturating_add(expiry_delta);

        let token_program_a = if which_token_program_a % 2 == 0 {
            Self::TOKEN_PROGRAM
        } else {
            Self::TOKEN_PROGRAM_2022
        };
        let token_program_b = if which_token_program_b % 2 == 0 {
            Self::TOKEN_PROGRAM
        } else {
            Self::TOKEN_PROGRAM_2022
        };

        // When which_swap_dvp is even, derive real PDAs from the instruction
        // args so the transaction can actually land. Otherwise use random actor
        // keys to exercise the InvalidSeeds rejection path.
        let (mint_a, mint_b, swap_dvp, nonce_tombstone, dvp_ata_a, dvp_ata_b) =
            if which_swap_dvp % 2 == 0 {
                let ma = if !self.mints.is_empty() {
                    self.mints[which_mint_a as usize % self.mints.len()]
                } else {
                    Pubkey::new_from_array(self.actor_key(which_mint_a))
                };
                let mb = if self.mints.len() >= 2 {
                    self.mints[(which_mint_b as usize + 1) % self.mints.len()]
                } else {
                    Pubkey::new_from_array(self.actor_key(which_mint_b))
                };
                let (dvp, _bump) = Self::derive_swap_dvp(
                    &self.program_id,
                    &settlement_authority_pubkey,
                    &user_a,
                    &user_b,
                    &ma,
                    &mb,
                    nonce,
                );
                let (tomb, _) =
                    Pubkey::find_program_address(&[b"nonce", dvp.as_ref()], &self.program_id);
                let ata_a = Self::derive_ata_with_program(&dvp, &ma, &token_program_a);
                let ata_b = Self::derive_ata_with_program(&dvp, &mb, &token_program_b);
                (ma, mb, dvp, tomb, ata_a, ata_b)
            } else {
                // Random addresses — exercise rejection paths (InvalidSeeds, etc.)
                let ma = Pubkey::new_from_array(self.actor_key(which_mint_a));
                let mb = Pubkey::new_from_array(self.actor_key(which_mint_b));
                let dvp = Pubkey::new_from_array(self.actor_key(which_swap_dvp));
                let tomb = Pubkey::new_from_array(self.actor_key(which_nonce_tombstone));
                let ata_a = Pubkey::new_from_array(self.actor_key(which_dvp_ata_a));
                let ata_b = Pubkey::new_from_array(self.actor_key(which_dvp_ata_b));
                (ma, mb, dvp, tomb, ata_a, ata_b)
            };
        let associated_token_program = Self::ATA_PROGRAM;
        self.watch(payer.pubkey());
        self.watch(swap_dvp);
        self.watch(nonce_tombstone);
        self.watch(settlement_authority_pubkey);
        self.watch(user_a);
        self.watch(user_b);
        self.watch(mint_a);
        self.watch(mint_b);
        self.watch(dvp_ata_a);
        self.watch(dvp_ata_b);
        self.watch(token_program_a);
        self.watch(token_program_b);
        self.watch(associated_token_program);
        let ref_string_value = [
            String::new(),
            String::from("a"),
            String::from("vaultmind"),
            "x".repeat(64),
        ][ref_string_value_choice as usize % 4]
            .clone();
        let ref_string = if ref_string_present % 2 == 1 {
            Some(ref_string_value)
        } else {
            None
        };
        let user_a_settlement_destination = if user_a_settlement_destination_present % 2 == 1 {
            Some(self.actor_key(user_a_settlement_destination_value_actor))
        } else {
            None
        };
        let user_b_settlement_destination = if user_b_settlement_destination_present % 2 == 1 {
            Some(self.actor_key(user_b_settlement_destination_value_actor))
        } else {
            None
        };
        self.snapshot();
        self.last = Action::new(
            "create_dvp",
            vec![
                ("amount_a", amount_a as u128),
                ("amount_b", amount_b as u128),
                ("expiry_timestamp", expiry_timestamp as u128),
                ("nonce", nonce as u128),
                ("ref_string_value_choice", ref_string_value_choice as u128),
                ("ref_string_present", ref_string_present as u128),
                (
                    "user_a_settlement_destination_value_actor",
                    user_a_settlement_destination_value_actor as u128,
                ),
                (
                    "user_a_settlement_destination_present",
                    user_a_settlement_destination_present as u128,
                ),
                (
                    "user_b_settlement_destination_value_actor",
                    user_b_settlement_destination_value_actor as u128,
                ),
                (
                    "user_b_settlement_destination_present",
                    user_b_settlement_destination_present as u128,
                ),
            ],
        );
        self.last.accounts = vec![
            ("payer", payer.pubkey()),
            ("swap_dvp", swap_dvp),
            ("nonce_tombstone", nonce_tombstone),
            ("settlement_authority", settlement_authority_pubkey),
            ("user_a", user_a),
            ("user_b", user_b),
            ("mint_a", mint_a),
            ("mint_b", mint_b),
            ("dvp_ata_a", dvp_ata_a),
            ("dvp_ata_b", dvp_ata_b),
            ("token_program_a", token_program_a),
            ("token_program_b", token_program_b),
            ("associated_token_program", associated_token_program),
        ];
        let outcome = self
            .ctx
            .raw_call(build_create_dvp_ix(
                self.program_id,
                payer.pubkey(),
                swap_dvp,
                nonce_tombstone,
                settlement_authority_pubkey,
                user_a,
                user_b,
                mint_a,
                mint_b,
                dvp_ata_a,
                dvp_ata_b,
                token_program_a,
                token_program_b,
                associated_token_program,
                amount_a,
                amount_b,
                expiry_timestamp,
                nonce,
                ref_string,
                user_a_settlement_destination,
                user_b_settlement_destination,
                earliest_settlement_timestamp,
            ))
            .signers(&[&*payer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("create_dvp");
        }
        if !self.last.ok {
            note_blocked(
                "create_dvp",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// `reclaim_dvp`. False if the transaction was rejected.
    #[allow(non_snake_case)]
    pub fn action_PROGRAM_reclaim_dvp(
        &mut self,
        which_signer: u64,
        which_swap_dvp: u64,
        which_mint: u64,
        which_dvp_source_ata: u64,
        which_signer_dest_ata: u64,
        which_token_program: u64,
        which_memo_program: u64,
    ) -> bool {
        let signer = match self.actor(which_signer) {
            Some(actor) => actor,
            None => {
                note_blocked("reclaim_dvp", -1, &"no signer available");
                return false;
            }
        };
        let (swap_dvp, mint, dvp_source_ata, token_program, signer_dest_ata) =
            if which_swap_dvp % 4 == 0 && !self.dvps.is_empty() {
                let d = &self.dvps[which_swap_dvp as usize % self.dvps.len()];
                // signer determines which leg
                let (m, ata, tp) = if signer.pubkey() == d.user_a {
                    (d.mint_a, d.dvp_ata_a, d.token_program_a)
                } else {
                    (d.mint_b, d.dvp_ata_b, d.token_program_b)
                };
                let dest = Self::derive_ata_with_program(&signer.pubkey(), &m, &tp);
                (d.swap_dvp, m, ata, tp, dest)
            } else {
                (
                    Pubkey::new_from_array(self.actor_key(which_swap_dvp)),
                    Pubkey::new_from_array(self.actor_key(which_mint)),
                    Pubkey::new_from_array(self.actor_key(which_dvp_source_ata)),
                    Self::TOKEN_PROGRAM,
                    Pubkey::new_from_array(self.actor_key(which_signer_dest_ata)),
                )
            };

        let memo_program = Self::MEMO_PROGRAM;
        self.watch(signer.pubkey());
        self.watch(swap_dvp);
        self.watch(mint);
        self.watch(dvp_source_ata);
        self.watch(signer_dest_ata);
        self.watch(token_program);
        self.watch(memo_program);
        self.snapshot();
        self.last = Action::new("reclaim_dvp", vec![]);
        self.last.accounts = vec![
            ("signer", signer.pubkey()),
            ("swap_dvp", swap_dvp),
            ("mint", mint),
            ("dvp_source_ata", dvp_source_ata),
            ("signer_dest_ata", signer_dest_ata),
            ("token_program", token_program),
            ("memo_program", memo_program),
        ];
        let outcome = self
            .ctx
            .raw_call(build_reclaim_dvp_ix(
                self.program_id,
                signer.pubkey(),
                swap_dvp,
                mint,
                dvp_source_ata,
                signer_dest_ata,
                token_program,
                memo_program,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("reclaim_dvp");
        }
        if !self.last.ok {
            note_blocked(
                "reclaim_dvp",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// `settle_dvp`. False if the transaction was rejected.
    #[allow(non_snake_case)]
    pub fn action_PROGRAM_settle_dvp(
        &mut self,
        which_settlement_authority: u64,
        which_swap_dvp: u64,
        which_mint_a: u64,
        which_mint_b: u64,
        which_dvp_ata_a: u64,
        which_dvp_ata_b: u64,
        which_user_a_destination_ata_b: u64,
        which_user_b_destination_ata_a: u64,
        which_user_a_ata_a: u64,
        which_user_b_ata_b: u64,
        #[range(0..4)] leg_a_extras_count: u8,
    ) -> bool {
        let settlement_authority = match self.actor(which_settlement_authority) {
            Some(actor) => actor,
            None => {
                note_blocked("settle_dvp", -1, &"no settlement auth available");
                return false;
            }
        };
        let (swap_dvp, mint_a, mint_b, dvp_ata_a, dvp_ata_b, token_program_a, token_program_b) =
            if which_swap_dvp % 4 == 0 && !self.dvps.is_empty() {
                let d = &self.dvps[which_swap_dvp as usize % self.dvps.len()];
                (
                    d.swap_dvp,
                    d.mint_a,
                    d.mint_b,
                    d.dvp_ata_a,
                    d.dvp_ata_b,
                    d.token_program_a,
                    d.token_program_b,
                )
            } else {
                (
                    Pubkey::new_from_array(self.actor_key(which_swap_dvp)),
                    Pubkey::new_from_array(self.actor_key(which_mint_a)),
                    Pubkey::new_from_array(self.actor_key(which_mint_b)),
                    Pubkey::new_from_array(self.actor_key(which_dvp_ata_a)),
                    Pubkey::new_from_array(self.actor_key(which_dvp_ata_b)),
                    Self::TOKEN_PROGRAM,
                    Self::TOKEN_PROGRAM,
                )
            };
        // When a real dvp record is selected, derive destination ATAs correctly
        // so the instruction can actually pass the canonicality checks. Otherwise
        // use random actor keys to exercise the rejection paths.
        let (user_a_destination_ata_b, user_b_destination_ata_a, user_a_ata_a, user_b_ata_b) =
            if which_swap_dvp % 4 == 0 && !self.dvps.is_empty() {
                let d = &self.dvps[which_swap_dvp as usize % self.dvps.len()];
                (
                    Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_b),
                    Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_a),
                    Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a),
                    Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b),
                )
            } else {
                (
                    Pubkey::new_from_array(self.actor_key(which_user_a_destination_ata_b)),
                    Pubkey::new_from_array(self.actor_key(which_user_b_destination_ata_a)),
                    Pubkey::new_from_array(self.actor_key(which_user_a_ata_a)),
                    Pubkey::new_from_array(self.actor_key(which_user_b_ata_b)),
                )
            };

        let memo_program = Self::MEMO_PROGRAM;
        self.watch(settlement_authority.pubkey());
        self.watch(swap_dvp);
        self.watch(mint_a);
        self.watch(mint_b);
        self.watch(dvp_ata_a);
        self.watch(dvp_ata_b);
        self.watch(user_a_destination_ata_b);
        self.watch(user_b_destination_ata_a);
        self.watch(user_a_ata_a);
        self.watch(user_b_ata_b);
        self.watch(token_program_a);
        self.watch(token_program_b);
        self.watch(memo_program);
        self.snapshot();
        self.last = Action::new(
            "settle_dvp",
            vec![("leg_a_extras_count", leg_a_extras_count as u128)],
        );
        self.last.accounts = vec![
            ("settlement_authority", settlement_authority.pubkey()),
            ("swap_dvp", swap_dvp),
            ("mint_a", mint_a),
            ("mint_b", mint_b),
            ("dvp_ata_a", dvp_ata_a),
            ("dvp_ata_b", dvp_ata_b),
            ("user_a_destination_ata_b", user_a_destination_ata_b),
            ("user_b_destination_ata_a", user_b_destination_ata_a),
            ("user_a_ata_a", user_a_ata_a),
            ("user_b_ata_b", user_b_ata_b),
            ("token_program_a", token_program_a),
            ("token_program_b", token_program_b),
            ("memo_program", memo_program),
        ];
        let outcome = self
            .ctx
            .raw_call(build_settle_dvp_ix(
                self.program_id,
                settlement_authority.pubkey(),
                swap_dvp,
                mint_a,
                mint_b,
                dvp_ata_a,
                dvp_ata_b,
                user_a_destination_ata_b,
                user_b_destination_ata_a,
                user_a_ata_a,
                user_b_ata_b,
                token_program_a,
                token_program_b,
                memo_program,
                leg_a_extras_count,
            ))
            .signers(&[&*settlement_authority])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("settle_dvp");
        }
        if !self.last.ok {
            note_blocked(
                "settle_dvp",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// `cancel_dvp`. False if the transaction was rejected.
    #[allow(non_snake_case)]
    pub fn action_PROGRAM_cancel_dvp(
        &mut self,
        which_settlement_authority: u64,
        which_swap_dvp: u64,
        which_mint_a: u64,
        which_mint_b: u64,
        which_dvp_ata_a: u64,
        which_dvp_ata_b: u64,
        which_user_a_ata_a: u64,
        which_user_b_ata_b: u64,
        which_token_program_a: u64,
        which_token_program_b: u64,
        which_memo_program: u64,
        #[range(0..4)] leg_a_extras_count: u8,
    ) -> bool {
        let settlement_authority = match self.actor(which_settlement_authority) {
            Some(actor) => actor,
            None => {
                note_blocked("cancel_dvp", -1, &"no settlement_authority available");
                return false;
            }
        };
        let (
            swap_dvp,
            mint_a,
            mint_b,
            dvp_ata_a,
            dvp_ata_b,
            token_program_a,
            token_program_b,
            user_a_ata_a,
            user_b_ata_b,
        ) = if which_swap_dvp % 4 == 0 && !self.dvps.is_empty() {
            let d = &self.dvps[which_swap_dvp as usize % self.dvps.len()];
            (
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                d.token_program_a,
                d.token_program_b,
                Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a),
                Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b),
            )
        } else {
            (
                Pubkey::new_from_array(self.actor_key(which_swap_dvp)),
                Pubkey::new_from_array(self.actor_key(which_mint_a)),
                Pubkey::new_from_array(self.actor_key(which_mint_b)),
                Pubkey::new_from_array(self.actor_key(which_dvp_ata_a)),
                Pubkey::new_from_array(self.actor_key(which_dvp_ata_b)),
                Self::TOKEN_PROGRAM,
                Self::TOKEN_PROGRAM,
                Pubkey::new_from_array(self.actor_key(which_user_a_ata_a)),
                Pubkey::new_from_array(self.actor_key(which_user_b_ata_b)),
            )
        };

        let memo_program = Self::MEMO_PROGRAM;
        self.watch(settlement_authority.pubkey());
        self.watch(swap_dvp);
        self.watch(mint_a);
        self.watch(mint_b);
        self.watch(dvp_ata_a);
        self.watch(dvp_ata_b);
        self.watch(user_a_ata_a);
        self.watch(user_b_ata_b);
        self.watch(token_program_a);
        self.watch(token_program_b);
        self.watch(memo_program);
        self.snapshot();
        self.last = Action::new(
            "cancel_dvp",
            vec![("leg_a_extras_count", leg_a_extras_count as u128)],
        );
        self.last.accounts = vec![
            ("settlement_authority", settlement_authority.pubkey()),
            ("swap_dvp", swap_dvp),
            ("mint_a", mint_a),
            ("mint_b", mint_b),
            ("dvp_ata_a", dvp_ata_a),
            ("dvp_ata_b", dvp_ata_b),
            ("user_a_ata_a", user_a_ata_a),
            ("user_b_ata_b", user_b_ata_b),
            ("token_program_a", token_program_a),
            ("token_program_b", token_program_b),
            ("memo_program", memo_program),
        ];
        let outcome = self
            .ctx
            .raw_call(build_cancel_dvp_ix(
                self.program_id,
                settlement_authority.pubkey(),
                swap_dvp,
                mint_a,
                mint_b,
                dvp_ata_a,
                dvp_ata_b,
                user_a_ata_a,
                user_b_ata_b,
                token_program_a,
                token_program_b,
                memo_program,
                leg_a_extras_count,
            ))
            .signers(&[&*settlement_authority])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("cancel_dvp");
        }
        if !self.last.ok {
            note_blocked(
                "cancel_dvp",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// `reject_dvp`. False if the transaction was rejected.
    #[allow(non_snake_case)]
    pub fn action_PROGRAM_reject_dvp(
        &mut self,
        which_signer: u64,
        which_swap_dvp: u64,
        which_mint_a: u64,
        which_mint_b: u64,
        which_dvp_ata_a: u64,
        which_dvp_ata_b: u64,
        which_user_a_ata_a: u64,
        which_user_b_ata_b: u64,
        which_token_program_a: u64,
        which_token_program_b: u64,
        which_memo_program: u64,
        #[range(0..4)] leg_a_extras_count: u8,
    ) -> bool {
        let signer = match self.actor(which_signer) {
            Some(actor) => actor,
            None => {
                note_blocked("reject_dvp", -1, &"no signer available");
                return false;
            }
        };
        let (
            swap_dvp,
            mint_a,
            mint_b,
            dvp_ata_a,
            dvp_ata_b,
            token_program_a,
            token_program_b,
            user_a_ata_a,
            user_b_ata_b,
        ) = if which_swap_dvp % 4 == 0 && !self.dvps.is_empty() {
            let d = &self.dvps[which_swap_dvp as usize % self.dvps.len()];
            (
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                d.token_program_a,
                d.token_program_b,
                Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a),
                Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b),
            )
        } else {
            (
                Pubkey::new_from_array(self.actor_key(which_swap_dvp)),
                Pubkey::new_from_array(self.actor_key(which_mint_a)),
                Pubkey::new_from_array(self.actor_key(which_mint_b)),
                Pubkey::new_from_array(self.actor_key(which_dvp_ata_a)),
                Pubkey::new_from_array(self.actor_key(which_dvp_ata_b)),
                Self::TOKEN_PROGRAM,
                Self::TOKEN_PROGRAM,
                Pubkey::new_from_array(self.actor_key(which_user_a_ata_a)),
                Pubkey::new_from_array(self.actor_key(which_user_b_ata_b)),
            )
        };

        let memo_program = Self::MEMO_PROGRAM;
        self.watch(signer.pubkey());
        self.watch(swap_dvp);
        self.watch(mint_a);
        self.watch(mint_b);
        self.watch(dvp_ata_a);
        self.watch(dvp_ata_b);
        self.watch(user_a_ata_a);
        self.watch(user_b_ata_b);
        self.watch(token_program_a);
        self.watch(token_program_b);
        self.watch(memo_program);
        self.snapshot();
        self.last = Action::new(
            "reject_dvp",
            vec![("leg_a_extras_count", leg_a_extras_count as u128)],
        );
        self.last.accounts = vec![
            ("signer", signer.pubkey()),
            ("swap_dvp", swap_dvp),
            ("mint_a", mint_a),
            ("mint_b", mint_b),
            ("dvp_ata_a", dvp_ata_a),
            ("dvp_ata_b", dvp_ata_b),
            ("user_a_ata_a", user_a_ata_a),
            ("user_b_ata_b", user_b_ata_b),
            ("token_program_a", token_program_a),
            ("token_program_b", token_program_b),
            ("memo_program", memo_program),
        ];
        let outcome = self
            .ctx
            .raw_call(build_reject_dvp_ix(
                self.program_id,
                signer.pubkey(),
                swap_dvp,
                mint_a,
                mint_b,
                dvp_ata_a,
                dvp_ata_b,
                user_a_ata_a,
                user_b_ata_b,
                token_program_a,
                token_program_b,
                memo_program,
                leg_a_extras_count,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("reject_dvp");
        }
        if !self.last.ok {
            note_blocked(
                "reject_dvp",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// `recover_dvp`. False if the transaction was rejected.
    #[allow(non_snake_case)]
    pub fn action_PROGRAM_recover_dvp(
        &mut self,
        which_signer: u64,
        which_swap_dvp: u64,
        which_nonce_tombstone: u64,
        which_mint: u64,
        which_dvp_escrow_ata: u64,
        which_signer_dest_ata: u64,
        which_token_program: u64,
        which_memo_program: u64,
        settlement_authority_actor: u64,
        user_a_actor: u64,
        user_b_actor: u64,
        mint_a_actor: u64,
        mint_b_actor: u64,
        #[range(0..16)] nonce: u64,
    ) -> bool {
        // Add a small probability path that derives real addresses from the instruction data:
        let (swap_dvp, nonce_tombstone) = if which_swap_dvp % 4 == 0 {
            // Derive real PDA from the seed inputs so we get past InvalidSeeds
            let nonce_bytes = (nonce % 16).to_le_bytes();
            let derived = Pubkey::find_program_address(
                &[
                    b"dvp",
                    self.actor_key(settlement_authority_actor).as_ref(),
                    self.actor_key(user_a_actor).as_ref(),
                    self.actor_key(user_b_actor).as_ref(),
                    self.actor_key(mint_a_actor).as_ref(),
                    self.actor_key(mint_b_actor).as_ref(),
                    &nonce_bytes,
                ],
                &self.program_id,
            )
            .0;
            let tombstone =
                Pubkey::find_program_address(&[b"nonce", derived.as_ref()], &self.program_id).0;
            (derived, tombstone)
        } else {
            (
                Pubkey::new_from_array(self.actor_key(which_swap_dvp)),
                Pubkey::new_from_array(self.actor_key(which_nonce_tombstone)),
            )
        };
        let signer = match self.actor(which_signer) {
            Some(actor) => actor,
            None => {
                note_blocked("recover_dvp", -1, &"no signer available");
                return false;
            }
        };
        let mut mint = Pubkey::new_from_array(self.actor_key(mint_a_actor));
        if which_swap_dvp % 4 == 0 {
            mint = if signer.pubkey() == Pubkey::new_from_array(self.actor_key(user_a_actor)) {
                Pubkey::new_from_array(self.actor_key(mint_a_actor))
            } else {
                Pubkey::new_from_array(self.actor_key(mint_b_actor))
            };
            // use this mint instead of actor_key(which_mint)
        }

        let dvp_escrow_ata = Pubkey::new_from_array(self.actor_key(which_dvp_escrow_ata));
        let signer_dest_ata = Pubkey::new_from_array(self.actor_key(which_signer_dest_ata));
        let token_program = Self::TOKEN_PROGRAM;
        let memo_program = Self::MEMO_PROGRAM;
        self.watch(signer.pubkey());
        self.watch(swap_dvp);
        self.watch(nonce_tombstone);
        self.watch(mint);
        self.watch(dvp_escrow_ata);
        self.watch(signer_dest_ata);
        self.watch(token_program);
        self.watch(memo_program);
        self.snapshot();
        self.last = Action::new(
            "recover_dvp",
            vec![
                (
                    "settlement_authority_actor",
                    settlement_authority_actor as u128,
                ),
                ("user_a_actor", user_a_actor as u128),
                ("user_b_actor", user_b_actor as u128),
                ("mint_a_actor", mint_a_actor as u128),
                ("mint_b_actor", mint_b_actor as u128),
                ("nonce", (nonce % 16) as u128),
            ],
        );
        self.last.accounts = vec![
            ("signer", signer.pubkey()),
            ("swap_dvp", swap_dvp),
            ("nonce_tombstone", nonce_tombstone),
            ("mint", mint),
            ("dvp_escrow_ata", dvp_escrow_ata),
            ("signer_dest_ata", signer_dest_ata),
            ("token_program", token_program),
            ("memo_program", memo_program),
        ];
        let outcome = self
            .ctx
            .raw_call(build_recover_dvp_ix(
                self.program_id,
                signer.pubkey(),
                swap_dvp,
                nonce_tombstone,
                mint,
                dvp_escrow_ata,
                signer_dest_ata,
                token_program,
                memo_program,
                self.actor_key(settlement_authority_actor),
                self.actor_key(user_a_actor),
                self.actor_key(user_b_actor),
                self.actor_key(mint_a_actor),
                self.actor_key(mint_b_actor),
                nonce % 16,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("recover_dvp");
        }
        if !self.last.ok {
            note_blocked(
                "recover_dvp",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }
    /// cancel with wrong authority — hits SettlementAuthorityMismatch (added:mubariz)
    #[allow(non_snake_case)]
    pub fn action_MODEL_cancel_wrong_authority(
        &mut self,
        which_dvp: u64,
        which_wrong_auth: u64,
    ) -> bool {
        if self.dvps.is_empty() {
            note_blocked("cancel_wrong_auth", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        // pick an actor that is NOT the authority
        let n = self.actors.len() as u64;
        let mut wi = which_wrong_auth % n;
        while self.actors[wi as usize].pubkey() == d.authority {
            wi = (wi + 1) % n;
        }
        let wrong_auth = self.actors[wi as usize].clone();

        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        self.snapshot();
        self.last = Action::new("cancel_wrong_auth", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_cancel_dvp_ix(
                self.program_id,
                wrong_auth.pubkey(), // ← wrong authority
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_a,
                ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*wrong_auth])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        note_blocked(
            "cancel_wrong_auth",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }

    /// reject with wrong signer — hits SignerNotParty(added:mubariz)
    #[allow(non_snake_case)]
    pub fn action_MODEL_reject_wrong_signer(&mut self, which_dvp: u64, which_wrong: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("reject_wrong_signer", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let n = self.actors.len() as u64;
        let mut wi = which_wrong % n;
        while self.actors[wi as usize].pubkey() == d.user_a
            || self.actors[wi as usize].pubkey() == d.user_b
        {
            wi = (wi + 1) % n;
        }
        let wrong = self.actors[wi as usize].clone();
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        self.snapshot();
        self.last = Action::new("reject_wrong_signer", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_reject_dvp_ix(
                self.program_id,
                wrong.pubkey(), // ← not user_a or user_b
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_a,
                ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*wrong])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        note_blocked(
            "reject_wrong_signer",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }

    /// cancel with swapped mints — hits InvalidAccountData mint mismatch(added:mubariz)
    #[allow(non_snake_case)]
    pub fn action_MODEL_cancel_swapped_mints(&mut self, which_dvp: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("cancel_swapped_mints", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let authority = match self.actors.iter().find(|k| k.pubkey() == d.authority) {
            Some(k) => k.clone(),
            None => return false,
        };

        let ua_b = Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_b);
        let ub_a = Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_a);
        self.snapshot();
        self.last = Action::new("cancel_swapped_mints", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_cancel_dvp_ix(
                self.program_id,
                d.authority,
                d.swap_dvp,
                d.mint_b, // ← swapped
                d.mint_a, // ← swapped
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_b,
                ub_a,
                d.token_program_b, // ← swapped
                d.token_program_a, // ← swapped
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*authority])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        note_blocked(
            "cancel_swapped_mints",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }
    #[allow(non_snake_case)]
    pub fn action_RUNTIME_create_actor(&mut self) -> bool {
        self.snapshot();
        self.last = Action::new("create_actor", vec![]);
        if self.actors.len() >= MAX_ACTORS {
            note_blocked("create_actor", -2, &"actor limit reached (MAX_ACTORS)");
            return false;
        }
        let seed = (self.actors.len() as u8).wrapping_add(1);
        let keypair = Arc::new(Keypair::new_from_array([seed; 32]));
        let made = self
            .ctx
            .create_account()
            .pubkey(keypair.pubkey())
            .lamports(FUNDED)
            .owner(SYSTEM_PROGRAM)
            .create()
            .is_ok();
        if made {
            self.actors.push(keypair);
            note_landed("create_actor");
        } else {
            note_blocked("create_actor", -2, &"account already exists");
        }
        self.last.ok = made;
        made
    }

    #[allow(non_snake_case)]
    pub fn action_RUNTIME_fund(
        &mut self,
        from: u64,
        to: u64,
        #[range(1..10_000_000_000)] lamports: u64,
    ) -> bool {
        self.snapshot();
        self.last = Action::new("fund", vec![("lamports", lamports as u128)]);
        let (source, target) = match (self.actor(from), self.actor(to)) {
            (Some(a), Some(b)) => (a.pubkey(), b.pubkey()),
            _ => {
                note_blocked("fund", -1, &"no actor/signer available");
                return false;
            }
        };
        // Same actor twice is a no-op that would still look like a success, and
        // more than it holds would underflow the debit.
        let held = self
            .ctx
            .get_account(&source)
            .map(|a| a.lamports)
            .unwrap_or(0);
        if source == target || held < lamports {
            note_blocked("fund", -2, &"same actor or insufficient balance");
            return false;
        }
        // update_account hands the closure the DATA, not the account, so lamports
        // have to move via read_account -> mutate -> write_account.
        let moved = (|| -> bool {
            let mut src = match self.ctx.read_account(&source) {
                Ok(a) => a,
                Err(_) => return false,
            };
            let mut dst = match self.ctx.read_account(&target) {
                Ok(a) => a,
                Err(_) => return false,
            };
            src.lamports = src.lamports.saturating_sub(lamports);
            dst.lamports = dst.lamports.saturating_add(lamports);
            if self.ctx.write_account(&source, src).is_err() {
                return false;
            }
            self.ctx.write_account(&target, dst).is_ok()
        })();
        self.last.ok = moved;
        if moved {
            note_landed("fund");
        } else {
            note_blocked("fund", -2, &"read/write account failed");
        }
        moved
    }
    /*
        // reject version — signer is user_a
        pub fn action_MODEL_reject_swapped_mints(&mut self, which_dvp: u64) -> bool {
            if self.dvps.is_empty() {
                note_blocked("reject_swapped_mints", -1, &"no dvps yet");
                return false;
            }
            let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
            let user_a = match self.actors.iter().find(|k| k.pubkey() == d.user_a) {
                Some(k) => k.clone(),
                None => return false,
            };
            let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
            let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
            self.snapshot();
            self.last = Action::new("reject_swapped_mints", vec![]);
            let outcome = self
                .ctx
                .raw_call(build_reject_dvp_ix(
                    self.program_id,
                    d.user_a,
                    d.swap_dvp,
                    d.mint_b, // ← swapped
                    d.mint_a, // ← swapped
                    d.dvp_ata_a,
                    d.dvp_ata_b,
                    ua_a,
                    ub_b,
                    d.token_program_b, // ← swapped
                    d.token_program_a, // ← swapped
                    Self::MEMO_PROGRAM,
                    0,
                ))
                .signers(&[&*user_a])
                .send();
            self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
            note_blocked(
                "reject_swapped_mints",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
            false
        }
    */
    // settle version — signer is authority
    pub fn action_MODEL_settle_swapped_mints(&mut self, which_dvp: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("settle_swapped_mints", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let authority = match self.actors.iter().find(|k| k.pubkey() == d.authority) {
            Some(k) => k.clone(),
            None => return false,
        };
        let ua_b = Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_b);
        let ub_a = Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_a);
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        self.snapshot();
        self.last = Action::new("settle_swapped_mints", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_settle_dvp_ix(
                self.program_id,
                d.authority,
                d.swap_dvp,
                d.mint_b, // ← swapped
                d.mint_a, // ← swapped
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_b,
                ub_a,
                ua_a,
                ub_b,
                d.token_program_b, // ← swapped
                d.token_program_a, // ← swapped
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*authority])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        note_blocked(
            "settle_swapped_mints",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }
    /// Not in the IDL. Keep it if the program reads Clock.
    #[allow(non_snake_case)] //range was too small for some tests, so I increased it to 500_000
    pub fn action_RUNTIME_warp_slots(&mut self, #[range(1..500_000)] slots: u64) -> bool {
        self.snapshot();
        self.last = Action::new("advance_slots", vec![("slots", slots as u128)]);
        self.last.ok = true;
        let next = self.ctx.slot() + slots;
        self.ctx.warp_to_slot(next);
        true
    }

    /// Create two SPL-Token mints (index 0 and 1) owned by the real token program.
    /// Nothing downstream of create_dvp can work without token-owned mints.
    #[allow(non_snake_case)]
    pub fn action_MODEL_create_mint_pair(&mut self) -> bool {
        self.snapshot();
        self.last = Action::new("model_create_mint_pair", vec![]);
        let authority = match self.actor(0) {
            Some(a) => a.pubkey(),
            None => {
                note_blocked("create_mint_pair", -1, &"no actor/signer yet");
                return false;
            }
        };
        let mut made = 0;
        for i in 0..2u8 {
            let mint = Pubkey::new_from_array([100u8 + i; 32]);
            if self
                .ctx
                .get_account(&mint)
                .map(|a| a.data.len() >= 82)
                .unwrap_or(false)
            {
                made += 1;
                self.mints.retain(|m| *m != mint);
                self.mints.push(mint);
                self.watch(mint);
                continue;
            }
            let ok = self
                .ctx
                .create_mint()
                .pubkey(mint)
                .mint_authority(authority)
                .decimals(0)
                .create()
                .is_ok();
            if ok {
                made += 1;
                self.mints.retain(|m| *m != mint);
                self.mints.push(mint);
                self.watch(mint);
            }
        }
        // Ensure every mint already in self.mints (SPL or T22, including
        // blocked mints) is watched and deduplicated — no separate fields needed.
        for &mint in &self.mints.clone() {
            if self
                .ctx
                .get_account(&mint)
                .map(|a| a.data.len() >= 82)
                .unwrap_or(false)
            {
                self.watch(mint);
            }
        }
        self.last.accounts = self
            .mints
            .iter()
            .enumerate()
            .map(|(i, m)| (if i == 0 { "mint_a" } else { "mint_b" }, *m))
            .collect();
        self.last.ok = made == 2;
        if self.last.ok {
            note_landed("create_mint_pair");
        } else {
            note_blocked("create_mint_pair", -2, &"mint creation failed");
        }
        self.last.ok
    }
    //added:mubariz
    #[allow(non_snake_case)]
    pub fn action_MODEL_settle_expired(&mut self, which_dvp: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("settle_expired", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let authority = match self.actors.iter().find(|k| k.pubkey() == d.authority) {
            Some(k) => k.clone(),
            None => return false,
        };
        // Warp well past expiry. The SVM clock advances ~0.4 s per slot, so
        // 365 days = 31_536_000 s needs ~78_840_000 slots. Use 80_000_000 to
        // guarantee unix_now() > expiry_timestamp for any DVP in the fixture.
        let next = self.ctx.slot() + 80_000_000;
        self.ctx.warp_to_slot(next);
        let ua_b = Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_b);
        let ub_a = Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_a);
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        self.snapshot();
        self.last = Action::new("settle_expired", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_settle_dvp_ix(
                self.program_id,
                d.authority,
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_b,
                ub_a,
                ua_a,
                ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*authority])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        note_blocked(
            "settle_expired",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }
    pub fn action_MODEL_create_dvp_real(
        &mut self,
        which_payer: u64,
        which_auth: u64,
        which_user_a: u64,
        which_user_b: u64,
        #[range(1..1_000_000)] amount_a: u64,
        #[range(1..1_000_000)] amount_b: u64,
        #[range(60..(365 * 24 * 60 * 60))] expiry_delta: i64,
        #[range(0..16)] nonce: u64,
        which_token_program: u64,
        user_a_dest_present: u64,
        user_b_dest_present: u64,
        earliest_settlement_timestamp: Option<i64>,
    ) -> bool {
        if self.actors.len() < 3 || self.mints.len() < 2 {
            note_blocked("create_dvp_real", -1, &"low actors or mints ");
            return false;
        }
        let payer = match self.actor(which_payer) {
            Some(a) => a,
            None => {
                note_blocked("create_dvp_real", -1, &"");
                return false;
            }
        };
        // three distinct actors: authority, user_a, user_b
        let n = self.actors.len() as u64;
        let ai = which_auth % n;
        let mut bi = which_user_a % n;
        if bi == ai {
            bi = (bi + 1) % n;
        }
        let mut ci = which_user_b % n;
        while ci == ai || ci == bi {
            ci = (ci + 1) % n;
        }
        let authority = self.actors[ai as usize].pubkey();
        let user_a = self.actors[bi as usize].pubkey();
        let user_b = self.actors[ci as usize].pubkey();

        let (mint_a, mint_b, token_program) =
            if which_token_program % 2 == 0 || self.mints.len() < 4 {
                (self.mints[0], self.mints[1], Self::TOKEN_PROGRAM)
            } else {
                (self.mints[2], self.mints[3], Self::TOKEN_PROGRAM_2022)
            };
        let (swap_dvp, _bump) = Self::derive_swap_dvp(
            &self.program_id,
            &authority,
            &user_a,
            &user_b,
            &mint_a,
            &mint_b,
            nonce,
        );
        let (tombstone, _tb) =
            Pubkey::find_program_address(&[b"nonce", swap_dvp.as_ref()], &self.program_id);
        let dvp_ata_a = Self::derive_ata_with_program(&swap_dvp, &mint_a, &token_program);
        let dvp_ata_b = Self::derive_ata_with_program(&swap_dvp, &mint_b, &token_program);
        let now = self.unix_now();
        let expiry = now.saturating_add(expiry_delta);

        for k in [
            payer.pubkey(),
            swap_dvp,
            tombstone,
            authority,
            user_a,
            user_b,
            mint_a,
            mint_b,
            dvp_ata_a,
            dvp_ata_b,
        ] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new(
            "create_dvp",
            vec![
                ("amount_a", amount_a as u128),
                ("amount_b", amount_b as u128),
                ("expiry_timestamp", expiry as u128),
                ("nonce", nonce as u128),
            ],
        );
        self.last.accounts = vec![
            ("payer", payer.pubkey()),
            ("swap_dvp", swap_dvp),
            ("nonce_tombstone", tombstone),
            ("settlement_authority", authority),
            ("user_a", user_a),
            ("user_b", user_b),
            ("mint_a", mint_a),
            ("mint_b", mint_b),
            ("dvp_ata_a", dvp_ata_a),
            ("dvp_ata_b", dvp_ata_b),
            ("token_program_a", token_program),
            ("token_program_b", token_program),
            ("associated_token_program", Self::ATA_PROGRAM),
        ];
        let user_a_dest = if user_a_dest_present % 2 == 1 {
            Some(self.actor_key(which_user_a)) // pass user_a as their own destination
        } else {
            None
        };
        let user_b_dest = if user_b_dest_present % 2 == 1 {
            Some(self.actor_key(which_user_b)) // pass user_b as their own destination
        } else {
            None
        };
        let outcome = self
            .ctx
            .raw_call(build_create_dvp_ix(
                self.program_id,
                payer.pubkey(),
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
                Self::ATA_PROGRAM,
                amount_a,
                amount_b,
                expiry,
                nonce,
                None,
                user_a_dest,
                user_b_dest,
                earliest_settlement_timestamp,
            ))
            .signers(&[&*payer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("create_dvp_real");
            //todo:check retain or not
            //  self.dvps.retain(|d| d.swap_dvp != swap_dvp);
            if self.dvps.len() < 8 {
                self.dvps.push(DvpRecord {
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
                });
            }
        } else {
            note_blocked(
                "create_dvp_real",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// Put tokens into both escrow ATAs of a created DvP, and create each
    /// party's own ATAs, by writing token accounts directly. Settle/Cancel/
    /// Reject/Reclaim all need funded escrows and existing destination ATAs.
    #[allow(non_snake_case)]
    pub fn action_MODEL_fund_legs(
        &mut self,
        which_dvp: u64,
        #[range(0..4_000_000)] extra: u64,
    ) -> bool {
        if self.dvps.is_empty() {
            note_blocked("fund_legs", -1, &"no  dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        self.snapshot();
        self.last = Action::new("model_fund_legs", vec![("extra", extra as u128)]);
        let amt_a = d.amount_a.saturating_add(extra % 3);
        let amt_b = d.amount_b.saturating_add(extra % 5);
        let mut ok = true;
        if d.token_program_a == Self::TOKEN_PROGRAM_2022 {
            note_blocked("fund_legs", -1, &"use fund_legs_tx for T22 dvps");
            return false;
        }
        // escrows must already exist (created by create_dvp); top them up
        ok &= self.set_token_amount(&d.dvp_ata_a, amt_a);
        ok &= self.set_token_amount(&d.dvp_ata_b, amt_b);
        // party + destination ATAs
        for (owner, mint) in [
            (d.user_a, d.mint_a),
            (d.user_a, d.mint_b),
            (d.user_b, d.mint_a),
            (d.user_b, d.mint_b),
        ] {
            let token_prog = if mint == d.mint_a {
                d.token_program_a
            } else {
                d.token_program_b
            };
            let ata = Self::derive_ata_with_program(&owner, &mint, &token_prog);
            if self
                .ctx
                .get_account(&ata)
                .map(|a| a.data.len() >= 72)
                .unwrap_or(false)
            {
                self.watch(ata);
                continue;
            }
            if token_prog == Self::TOKEN_PROGRAM_2022 {
                let mut data = vec![0u8; 165];
                data[0..32].copy_from_slice(&mint.to_bytes());
                data[32..64].copy_from_slice(&owner.to_bytes());
                data[64..72].copy_from_slice(&0u64.to_le_bytes());
                data[108] = 1;
                let account = solana_account::Account {
                    lamports: 2_039_280,
                    data,
                    owner: Self::TOKEN_PROGRAM_2022,
                    executable: false,
                    rent_epoch: u64::MAX,
                };
                ok &= self.ctx.svm.set_account(ata, account.into()).is_ok();
            } else {
                ok &= self
                    .ctx
                    .create_token_account()
                    .pubkey(ata)
                    .mint(mint)
                    .token_owner(owner)
                    .amount(0)
                    .create()
                    .is_ok();
            }

            self.watch(ata);
        }
        self.last.accounts = vec![
            ("swap_dvp", d.swap_dvp),
            ("dvp_ata_a", d.dvp_ata_a),
            ("dvp_ata_b", d.dvp_ata_b),
        ];
        self.last.ok = ok;
        if ok {
            note_landed("fund_legs");
        } else {
            note_blocked("fund_legs", -2, &"set_token_amount or ATA creation failed");
        }
        ok
    }

    /// settle_dvp against a DvP this fixture created, with all ten accounts
    /// derived the way the program re-derives them.
    #[allow(non_snake_case)]
    pub fn action_MODEL_settle_real(
        &mut self,
        which_dvp: u64,
        #[range(0..4)] leg_a_extras_count: u8,
    ) -> bool {
        if self.dvps.is_empty() {
            note_blocked("settle_real", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        // Guard: swap_dvp must still be program-owned (not already settled/rejected).
        if !self
            .ctx
            .get_account(&d.swap_dvp)
            .map(|a| !a.data.is_empty())
            .unwrap_or(false)
        {
            note_blocked("settle_real", -1, &"swap_dvp already closed");
            return false;
        }
        // Guard: both escrows must hold at least the agreed amounts.
        let bal_a = self.token_amount(&d.dvp_ata_a).unwrap_or(0);
        let bal_b = self.token_amount(&d.dvp_ata_b).unwrap_or(0);
        if bal_a < d.amount_a || bal_b < d.amount_b {
            note_blocked("settle_real", -1, &"leg not funded");
            return false;
        }
        let authority = match self.actors.iter().find(|k| k.pubkey() == d.authority) {
            Some(k) => k.clone(),
            None => {
                note_blocked("settle_real", -1, &"authority keypair not found");
                return false;
            }
        };

        let ua_dest_b = Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_b);
        let ub_dest_a = Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_a);
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        for (ata, mint, owner, token_prog) in [
            (ua_dest_b, d.mint_b, d.user_a, d.token_program_b),
            (ub_dest_a, d.mint_a, d.user_b, d.token_program_a),
            (ua_a, d.mint_a, d.user_a, d.token_program_a),
            (ub_b, d.mint_b, d.user_b, d.token_program_b),
        ] {
            if !self
                .ctx
                .get_account(&ata)
                .map(|a| a.data.len() >= 72)
                .unwrap_or(false)
            {
                if token_prog == Self::TOKEN_PROGRAM_2022 {
                    let mut data = vec![0u8; 165];
                    data[0..32].copy_from_slice(&mint.to_bytes());
                    data[32..64].copy_from_slice(&owner.to_bytes());
                    data[64..72].copy_from_slice(&0u64.to_le_bytes());
                    data[108] = 1;
                    let account = solana_account::Account {
                        lamports: 2_039_280,
                        data,
                        owner: Self::TOKEN_PROGRAM_2022,
                        executable: false,
                        rent_epoch: u64::MAX,
                    };
                    let _ = self.ctx.svm.set_account(ata, account.into());
                } else {
                    let _ = self
                        .ctx
                        .create_token_account()
                        .pubkey(ata)
                        .mint(mint)
                        .token_owner(owner)
                        .amount(0)
                        .create();
                }
            }
        }
        for k in [
            d.swap_dvp,
            d.dvp_ata_a,
            d.dvp_ata_b,
            ua_dest_b,
            ub_dest_a,
            ua_a,
            ub_b,
        ] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new(
            "settle_dvp",
            vec![("leg_a_extras_count", leg_a_extras_count as u128)],
        );
        self.last.accounts = vec![
            ("settlement_authority", d.authority),
            ("swap_dvp", d.swap_dvp),
            ("mint_a", d.mint_a),
            ("mint_b", d.mint_b),
            ("dvp_ata_a", d.dvp_ata_a),
            ("dvp_ata_b", d.dvp_ata_b),
            ("user_a_destination_ata_b", ua_dest_b),
            ("user_b_destination_ata_a", ub_dest_a),
            ("user_a_ata_a", ua_a),
            ("user_b_ata_b", ub_b),
            ("token_program_a", d.token_program_a),
            ("token_program_b", d.token_program_b),
            ("memo_program", Self::MEMO_PROGRAM),
        ];
        let outcome = self
            .ctx
            .raw_call(build_settle_dvp_ix(
                self.program_id,
                d.authority,
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_dest_b,
                ub_dest_a,
                ua_a,
                ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                leg_a_extras_count,
            ))
            .signers(&[&*authority])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("settle_real");
            self.dvps.retain(|x| x.swap_dvp != d.swap_dvp);
        } else {
            note_blocked(
                "settle_real",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// cancel_dvp on a DvP this fixture created, signed by its authority.
    #[allow(non_snake_case)]
    pub fn action_MODEL_cancel_real(&mut self, which_dvp: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("cancel_real", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        // Guard: swap_dvp must still be program-owned.
        if !self
            .ctx
            .get_account(&d.swap_dvp)
            .map(|a| !a.data.is_empty())
            .unwrap_or(false)
        {
            note_blocked("cancel_real", -1, &"swap_dvp already closed");
            return false;
        }
        let authority = match self.actors.iter().find(|k| k.pubkey() == d.authority) {
            Some(k) => k.clone(),
            None => {
                note_blocked("cancel_real", -1, &"authority keypair not found");
                return false;
            }
        };
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        for k in [d.swap_dvp, d.dvp_ata_a, d.dvp_ata_b, ua_a, ub_b] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new("cancel_dvp", vec![("leg_a_extras_count", 0u128)]);
        self.last.accounts = vec![
            ("settlement_authority", d.authority),
            ("swap_dvp", d.swap_dvp),
            ("mint_a", d.mint_a),
            ("mint_b", d.mint_b),
            ("dvp_ata_a", d.dvp_ata_a),
            ("dvp_ata_b", d.dvp_ata_b),
            ("user_a_ata_a", ua_a),
            ("user_b_ata_b", ub_b),
            ("token_program_a", d.token_program_a),
            ("token_program_b", d.token_program_b),
            ("memo_program", Self::MEMO_PROGRAM),
        ];
        let outcome = self
            .ctx
            .raw_call(build_cancel_dvp_ix(
                self.program_id,
                d.authority,
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_a,
                ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*authority])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("cancel_real");
            self.dvps.retain(|x| x.swap_dvp != d.swap_dvp);
        } else {
            note_blocked(
                "cancel_real",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }
    //added:mubariz
    #[allow(non_snake_case)]
    pub fn action_MODEL_replenish_dvp(&mut self, #[range(8..64)] nonce: u64) -> bool {
        self.snapshot();
        self.last = Action::new("replenish_dvp", vec![("nonce", nonce as u128)]);
        if self.actors.len() < 3 || self.mints.len() < 2 {
            note_blocked("replenish_dvp", -1, &"not enough actors or mints");
            return false;
        }
        let authority = self.actors[0].pubkey();
        let user_a = self.actors[1].pubkey();
        let user_b = self.actors[2].pubkey();
        let mint_a = self.mints[0];
        let mint_b = self.mints[1];
        let amount_a = 1_000u64;
        let amount_b = 2_000u64;

        let (swap_dvp, _) = Self::derive_swap_dvp(
            &self.program_id,
            &authority,
            &user_a,
            &user_b,
            &mint_a,
            &mint_b,
            nonce,
        );
        let (tombstone, _) =
            Pubkey::find_program_address(&[b"nonce", swap_dvp.as_ref()], &self.program_id);
        let dvp_ata_a = Self::derive_ata_with_program(&swap_dvp, &mint_a, &Self::TOKEN_PROGRAM);
        let dvp_ata_b = Self::derive_ata_with_program(&swap_dvp, &mint_b, &Self::TOKEN_PROGRAM);

        // Skip if already exists
        if self
            .ctx
            .get_account(&swap_dvp)
            .map(|a| !a.data.is_empty())
            .unwrap_or(false)
        {
            note_blocked("replenish_dvp", -2, &"dvp already exists");
            return false;
        }

        let now = self.unix_now();
        let expiry = now.saturating_add(365 * 24 * 60 * 60);

        let ix = build_create_dvp_ix(
            self.program_id,
            self.actors[0].pubkey(),
            swap_dvp,
            tombstone,
            authority,
            user_a,
            user_b,
            mint_a,
            mint_b,
            dvp_ata_a,
            dvp_ata_b,
            Self::TOKEN_PROGRAM,
            Self::TOKEN_PROGRAM,
            Self::ATA_PROGRAM,
            amount_a,
            amount_b,
            expiry,
            nonce,
            None,
            None,
            None,
            None,
        );
        let outcome = self.ctx.raw_call(ix).signers(&[&*self.actors[0]]).send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);

        if self.last.ok {
            // Fund it via real SPL transfers
            let ua_a = Self::derive_ata_with_program(&user_a, &mint_a, &Self::TOKEN_PROGRAM);
            let ub_b = Self::derive_ata_with_program(&user_b, &mint_b, &Self::TOKEN_PROGRAM);
            for (owner, mint, ata) in [(user_a, mint_a, ua_a), (user_b, mint_b, ub_b)] {
                if !self
                    .ctx
                    .get_account(&ata)
                    .map(|a| a.data.len() >= 72)
                    .unwrap_or(false)
                {
                    let _ = self
                        .ctx
                        .create_token_account()
                        .pubkey(ata)
                        .mint(mint)
                        .token_owner(owner)
                        .amount(0)
                        .create();
                }
            }
            for (mint, dest, amount) in [(mint_a, ua_a, amount_a), (mint_b, ub_b, amount_b)] {
                let mut data = vec![7u8];
                data.extend_from_slice(&amount.to_le_bytes());
                let ix = Instruction {
                    program_id: Self::TOKEN_PROGRAM,
                    accounts: vec![
                        AccountMeta::new(mint, false),
                        AccountMeta::new(dest, false),
                        AccountMeta::new_readonly(self.actors[0].pubkey(), true),
                    ],
                    data,
                };
                let _ = self.ctx.raw_call(ix).signers(&[&*self.actors[0]]).send();
            }

            for (src, dst, signer_idx, amount) in [
                (ua_a, dvp_ata_a, 1usize, amount_a),
                (ub_b, dvp_ata_b, 2usize, amount_b),
            ] {
                let mut data = vec![3u8];
                data.extend_from_slice(&amount.to_le_bytes());
                let ix = Instruction {
                    program_id: Self::TOKEN_PROGRAM,
                    accounts: vec![
                        AccountMeta::new(src, false),
                        AccountMeta::new(dst, false),
                        AccountMeta::new_readonly(self.actors[signer_idx].pubkey(), true),
                    ],
                    data,
                };
                let _ = self
                    .ctx
                    .raw_call(ix)
                    .signers(&[&*self.actors[signer_idx]])
                    .send();
            }

            if self.dvps.len() < 8 {
                self.dvps.push(DvpRecord {
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
                    token_program_a: Self::TOKEN_PROGRAM,
                    token_program_b: Self::TOKEN_PROGRAM,
                });
            }
            note_landed("replenish_dvp");
        } else {
            note_blocked(
                "replenish_dvp",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }
    /// todo:check dvp retian ,enabled token 22 usage ,check if dvp creation is correct
    #[allow(non_snake_case)]
    pub fn action_MODEL_reject_real(&mut self, which_dvp: u64, which_party: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("reject_real", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let party = if which_party % 2 == 0 {
            d.user_a
        } else {
            d.user_b
        };
        let signer = match self.actors.iter().find(|k| k.pubkey() == party) {
            Some(k) => k.clone(),
            None => {
                note_blocked("reject_real", -1, &"party keypair not found");
                return false;
            }
        };
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);

        for (ata, mint, owner, token_prog) in [
            (ua_a, d.mint_a, d.user_a, d.token_program_a),
            (ub_b, d.mint_b, d.user_b, d.token_program_b),
        ] {
            if !self
                .ctx
                .get_account(&ata)
                .map(|a| a.data.len() >= 72)
                .unwrap_or(false)
            {
                if token_prog == Self::TOKEN_PROGRAM_2022 {
                    let mut data = vec![0u8; 165];
                    data[0..32].copy_from_slice(&mint.to_bytes());
                    data[32..64].copy_from_slice(&owner.to_bytes());
                    data[64..72].copy_from_slice(&0u64.to_le_bytes());
                    data[108] = 1;
                    let account = solana_account::Account {
                        lamports: 2_039_280,
                        data,
                        owner: Self::TOKEN_PROGRAM_2022,
                        executable: false,
                        rent_epoch: u64::MAX,
                    };
                    let _ = self.ctx.svm.set_account(ata, account.into());
                } else {
                    let _ = self
                        .ctx
                        .create_token_account()
                        .pubkey(ata)
                        .mint(mint)
                        .token_owner(owner)
                        .amount(0)
                        .create();
                }
            }
        }
        for k in [d.swap_dvp, d.dvp_ata_a, d.dvp_ata_b, ua_a, ub_b] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new("reject_dvp", vec![("leg_a_extras_count", 0u128)]);
        self.last.accounts = vec![
            ("signer", party),
            ("swap_dvp", d.swap_dvp),
            ("mint_a", d.mint_a),
            ("mint_b", d.mint_b),
            ("dvp_ata_a", d.dvp_ata_a),
            ("dvp_ata_b", d.dvp_ata_b),
            ("user_a_ata_a", ua_a),
            ("user_b_ata_b", ub_b),
            ("token_program_a", d.token_program_a),
            ("token_program_b", d.token_program_b),
            ("memo_program", Self::MEMO_PROGRAM),
        ];
        let outcome = self
            .ctx
            .raw_call(build_reject_dvp_ix(
                self.program_id,
                party,
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_a,
                ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("reject_real");
            self.dvps.retain(|x| x.swap_dvp != d.swap_dvp);
        } else {
            note_blocked(
                "reject_real",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    /// enable token 22 usage:todo(check why t22 token account creation ,decrease coverage)
    #[allow(non_snake_case)]
    pub fn action_MODEL_reclaim_real(&mut self, which_dvp: u64, which_party: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("reclaim_real", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        // Guard: swap_dvp must still be program-owned.
        if !self
            .ctx
            .get_account(&d.swap_dvp)
            .map(|a| !a.data.is_empty())
            .unwrap_or(false)
        {
            note_blocked("reclaim_real", -1, &"swap_dvp already closed");
            return false;
        }
        let a_side = which_party % 2 == 0;
        let party = if a_side { d.user_a } else { d.user_b };
        let mint = if a_side { d.mint_a } else { d.mint_b };
        let escrow = if a_side { d.dvp_ata_a } else { d.dvp_ata_b };
        let signer = match self.actors.iter().find(|k| k.pubkey() == party) {
            Some(k) => k.clone(),
            None => {
                note_blocked("reclaim_real", -1, &"party keypair not found");
                return false;
            }
        };
        let token_program = if a_side {
            d.token_program_a
        } else {
            d.token_program_b
        };
        let dest = Self::derive_ata_with_program(&party, &mint, &token_program);
        for k in [d.swap_dvp, escrow, dest, mint] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new("reclaim_dvp", vec![]);
        self.last.accounts = vec![
            ("signer", party),
            ("swap_dvp", d.swap_dvp),
            ("mint", mint),
            ("dvp_source_ata", escrow),
            ("signer_dest_ata", dest),
            ("token_program", token_program),
            ("memo_program", Self::MEMO_PROGRAM),
        ];
        let outcome = self
            .ctx
            .raw_call(build_reclaim_dvp_ix(
                self.program_id,
                party,
                d.swap_dvp,
                mint,
                escrow,
                dest,
                token_program,
                Self::MEMO_PROGRAM,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("reclaim_real");
        } else {
            note_blocked(
                "reclaim_real",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }
    //enables T2022 recovery coverage by dynamically selecting  token program, manually constructing T22 accounts (165-byte layout),
    // also here using derive_ata_with_program for destination ATAs
    #[allow(non_snake_case)]
    pub fn action_MODEL_recover_real(
        &mut self,
        which_dvp: u64,
        which_party: u64,
        #[range(0..1_000)] preload: u64,
    ) -> bool {
        if self.closed.is_empty() {
            note_blocked("recover_real", -1, &"no closed dvps yet");
            return false;
        }
        let d = self.closed[(which_dvp as usize) % self.closed.len()].clone();
        let a_side = which_party % 2 == 0;
        let party = if a_side { d.user_a } else { d.user_b };
        let mint = if a_side { d.mint_a } else { d.mint_b };
        let escrow = if a_side { d.dvp_ata_a } else { d.dvp_ata_b };
        let signer = match self.actors.iter().find(|k| k.pubkey() == party) {
            Some(k) => k.clone(),
            None => {
                note_blocked("recover_real", -1, &"party keypair not found");
                return false;
            }
        };
        // The swap_dvp must be system-owned and empty for RecoverDvp; if some
        // other path left it live, this is the wrong instruction.
        if self
            .ctx
            .get_account(&d.swap_dvp)
            .map(|a| !a.data.is_empty())
            .unwrap_or(false)
        {
            note_blocked("recover_real", -1, &"swap_dvp still live");
            return false;
        }
        let token_program = if a_side {
            d.token_program_a
        } else {
            d.token_program_b
        };
        // recreate the escrow ATA for the dead PDA, as a late deposit would
        if !self
            .ctx
            .get_account(&escrow)
            .map(|a| a.data.len() >= 72)
            .unwrap_or(false)
        {
            if token_program == Self::TOKEN_PROGRAM_2022 {
                // Manually create a Token-2022 token account at escrow address
                let mut data = vec![0u8; 165];
                data[0..32].copy_from_slice(&mint.to_bytes()); // mint
                data[32..64].copy_from_slice(&d.swap_dvp.to_bytes()); // owner
                data[64..72].copy_from_slice(&preload.to_le_bytes()); // amount
                data[108] = 1; // is_initialized = true
                let account = solana_account::Account {
                    lamports: 2_039_280,
                    data,
                    owner: Self::TOKEN_PROGRAM_2022,
                    executable: false,
                    rent_epoch: u64::MAX,
                };
                let _ = self.ctx.svm.set_account(escrow, account.into());
            } else {
                let _ = self
                    .ctx
                    .create_token_account()
                    .pubkey(escrow)
                    .mint(mint)
                    .token_owner(d.swap_dvp)
                    .amount(preload)
                    .create();
            }
        }

        let dest = Self::derive_ata_with_program(&party, &mint, &token_program);
        if !self
            .ctx
            .get_account(&dest)
            .map(|a| a.data.len() >= 72)
            .unwrap_or(false)
        {
            if token_program == Self::TOKEN_PROGRAM_2022 {
                let mut data = vec![0u8; 165];
                data[0..32].copy_from_slice(&mint.to_bytes());
                data[32..64].copy_from_slice(&party.to_bytes()); // owner = party
                data[64..72].copy_from_slice(&0u64.to_le_bytes()); // amount = 0
                data[108] = 1; // is_initialized
                let account = solana_account::Account {
                    lamports: 2_039_280,
                    data,
                    owner: Self::TOKEN_PROGRAM_2022,
                    executable: false,
                    rent_epoch: u64::MAX,
                };
                let _ = self.ctx.svm.set_account(dest, account.into());
            } else {
                let _ = self
                    .ctx
                    .create_token_account()
                    .pubkey(dest)
                    .mint(mint)
                    .token_owner(party)
                    .amount(0)
                    .create();
            }
        }
        for k in [d.swap_dvp, d.tombstone, escrow, dest, mint] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new(
            "recover_dvp",
            vec![("nonce", d.nonce as u128), ("preload", preload as u128)],
        );
        self.last.accounts = vec![
            ("signer", party),
            ("swap_dvp", d.swap_dvp),
            ("nonce_tombstone", d.tombstone),
            ("mint", mint),
            ("dvp_escrow_ata", escrow),
            ("signer_dest_ata", dest),
            ("token_program", token_program),
            ("memo_program", Self::MEMO_PROGRAM),
        ];
        let outcome = self
            .ctx
            .raw_call(build_recover_dvp_ix(
                self.program_id,
                party,
                d.swap_dvp,
                d.tombstone,
                mint,
                escrow,
                dest,
                token_program,
                Self::MEMO_PROGRAM,
                d.authority.to_bytes(),
                d.user_a.to_bytes(),
                d.user_b.to_bytes(),
                d.mint_a.to_bytes(),
                d.mint_b.to_bytes(),
                d.nonce,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("recover_real");
        } else {
            note_blocked(
                "recover_real",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }

    #[allow(non_snake_case)]
    pub fn action_MODEL_remember_closed(&mut self) -> bool {
        self.snapshot();
        self.last = Action::new("model_remember_closed", vec![]);
        let mut moved = 0;
        let snapshot: Vec<DvpRecord> = self.dvps.clone();
        for d in snapshot {
            let live = self
                .ctx
                .get_account(&d.swap_dvp)
                .map(|a| !a.data.is_empty())
                .unwrap_or(false);
            if !live {
                self.dvps.retain(|x| x.swap_dvp != d.swap_dvp);
                if self.closed.len() < 8 && !self.closed.iter().any(|x| x.swap_dvp == d.swap_dvp) {
                    self.closed.push(d);
                    moved += 1;
                }
            }
        }
        self.last.ok = moved > 0;
        if self.last.ok {
            note_landed("remember_closed");
        } else {
            note_blocked("remember_closed", -2, &"no closed dvps to promote");
        }
        self.last.ok
    }
    //added:mubariz->Trying to create a DVP with a T22 mint that has a blocked extension (TransferFee, InterestBearing, NonTransferable) , always expects BlockedMintExtension (Custom(10))
    #[allow(non_snake_case)]
    pub fn action_MODEL_create_dvp_blocked_mint(
        &mut self,
        which_blocked: u64,
        which_leg: u64,
    ) -> bool {
        if self.actors.len() < 3 {
            return false;
        }

        let blocked_mints = [
            Pubkey::new_from_array([110u8; 32]),
            Pubkey::new_from_array([111u8; 32]),
            Pubkey::new_from_array([112u8; 32]),
            Pubkey::new_from_array([113u8; 32]),
        ];
        let blocked_mint = blocked_mints[(which_blocked as usize) % 4];

        let authority = self.actors[0].pubkey();
        let user_a = self.actors[1].pubkey();
        let user_b = self.actors[2].pubkey();
        let nonce = 99u64 + which_blocked % 4; // won't collide with setup nonces
                                               // First select mints
        let blocked_mint = blocked_mints[(which_blocked as usize) % 4];
        let (mint_a, mint_b, token_program_a, token_program_b) = if which_leg % 2 == 0 {
            (
                blocked_mint,
                self.mints[1],
                Self::TOKEN_PROGRAM_2022,
                Self::TOKEN_PROGRAM,
            )
        } else {
            (
                self.mints[0],
                blocked_mint,
                Self::TOKEN_PROGRAM,
                Self::TOKEN_PROGRAM_2022,
            )
        };

        // THEN derive swap_dvp using the selected mints
        let (swap_dvp, _) = Self::derive_swap_dvp(
            &self.program_id,
            &authority,
            &user_a,
            &user_b,
            &mint_a,
            &mint_b,
            nonce,
        );
        let (tombstone, _) =
            Pubkey::find_program_address(&[b"nonce", swap_dvp.as_ref()], &self.program_id);
        let dvp_ata_a = Self::derive_ata_with_program(&swap_dvp, &mint_a, &token_program_a);
        let dvp_ata_b = Self::derive_ata_with_program(&swap_dvp, &mint_b, &token_program_b);

        self.snapshot();
        self.last = Action::new(
            "create_dvp_blocked_mint",
            vec![("which_blocked", which_blocked as u128)],
        );

        let now = self.unix_now();
        let expiry = now.saturating_add(30 * 24 * 60 * 60);
        //   let blocked_mint = which_blocked % 4;

        let outcome = self
            .ctx
            .raw_call(build_create_dvp_ix(
                self.program_id,
                self.actors[0].pubkey(),
                swap_dvp,
                tombstone,
                authority,
                user_a,
                user_b,
                mint_a,
                mint_b,
                dvp_ata_a,
                dvp_ata_b,
                token_program_a,
                token_program_b,
                Self::ATA_PROGRAM,
                1_000,
                2_000,
                expiry,
                nonce,
                None,
                None,
                None,
                None,
            ))
            .signers(&[&*self.actors[0]])
            .send();

        self.last.ok = false; // always fails — that's the point
        note_blocked(
            "create_dvp_blocked_mint",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }
    //introducing t22 (which_token_program), enabling the fuzzer to exercise both SPL Token and Token-2022 code paths within a single action.  also extends expiry testing , to target the EarliestAfterExpiry guard, and stores explicit token_program_a/b in DvpRecord
    #[allow(non_snake_case)]
    pub fn action_MODEL_create_dvp_span(
        &mut self,
        which_payer: u64,
        which_auth: u64,
        which_user_a: u64,
        which_user_b: u64,
        #[range(0..6)] which_expiry: u64,
        #[range(0..8)] nonce: u64,
        which_token_program: u64,
    ) -> bool {
        if self.actors.len() < 3 || self.mints.len() < 2 {
            note_blocked("create_dvp_span", -1, &"not enough actors or mints");
            return false;
        }
        let payer = match self.actor(which_payer) {
            Some(a) => a,
            None => {
                note_blocked("create_dvp_span", -1, &"no payer available");
                return false;
            }
        };
        let n = self.actors.len() as u64;
        let ai = which_auth % n;
        let mut bi = which_user_a % n;
        if bi == ai {
            bi = (bi + 1) % n;
        }
        let mut ci = which_user_b % n;
        while ci == ai || ci == bi {
            ci = (ci + 1) % n;
        }
        let authority = self.actors[ai as usize].pubkey();
        let user_a = self.actors[bi as usize].pubkey();
        let user_b = self.actors[ci as usize].pubkey();
        //for exploring token22 branches
        let (mint_a, mint_b, token_program) = if which_token_program % 2 == 0 {
            (self.mints[0], self.mints[1], Self::TOKEN_PROGRAM)
        } else {
            // T22 mints — need at least 2 T22 mints available
            if self.mints.len() < 4 {
                note_blocked("create_dvp_span", -1, &"no t22 mints");
                return false;
            }
            (self.mints[2], self.mints[3], Self::TOKEN_PROGRAM_2022)
        };

        let now = self.unix_now();
        const YEAR: i64 = 365 * 24 * 60 * 60;
        let (expiry, earliest) = match which_expiry % 7 {
            // extend to 7,hit the EarliestAfterExpiry guard
            0 => (now.saturating_add(1), None),
            1 => (now.saturating_add(3_600), None),
            2 => (now.saturating_add(86_400), None),
            3 => (now.saturating_add(YEAR), None),
            4 => (now.saturating_add(YEAR / 2), None),
            5 => (now.saturating_add(86_400), Some(now.saturating_add(86_401))), // EarliestAfterExpiry
            _ => (1_i64, None),
        };

        let (swap_dvp, _b) = Self::derive_swap_dvp(
            &self.program_id,
            &authority,
            &user_a,
            &user_b,
            &mint_a,
            &mint_b,
            nonce,
        );
        let (tombstone, _tb) =
            Pubkey::find_program_address(&[b"nonce", swap_dvp.as_ref()], &self.program_id);
        let dvp_ata_a = Self::derive_ata_with_program(&swap_dvp, &mint_a, &token_program);
        let dvp_ata_b = Self::derive_ata_with_program(&swap_dvp, &mint_b, &token_program);
        for k in [
            payer.pubkey(),
            swap_dvp,
            tombstone,
            authority,
            user_a,
            user_b,
            mint_a,
            mint_b,
            dvp_ata_a,
            dvp_ata_b,
        ] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new(
            "create_dvp",
            vec![
                ("amount_a", 1_000u128),
                ("amount_b", 2_000u128),
                ("expiry_timestamp", expiry as u128),
                ("nonce", nonce as u128),
            ],
        );
        self.last.accounts = vec![
            ("payer", payer.pubkey()),
            ("swap_dvp", swap_dvp),
            ("nonce_tombstone", tombstone),
            ("settlement_authority", authority),
            ("user_a", user_a),
            ("user_b", user_b),
            ("mint_a", mint_a),
            ("mint_b", mint_b),
            ("dvp_ata_a", dvp_ata_a),
            ("dvp_ata_b", dvp_ata_b),
            ("token_program_a", token_program),
            ("token_program_b", token_program),
            ("memo_program", Self::MEMO_PROGRAM),
            ("associated_token_program", Self::ATA_PROGRAM),
        ];

        let outcome = self
            .ctx
            .raw_call(build_create_dvp_ix(
                self.program_id,
                payer.pubkey(),
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
                Self::ATA_PROGRAM,
                1_000,
                2_000,
                expiry,
                nonce,
                None,
                None,
                None,
                earliest,
            ))
            .signers(&[&*payer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        if self.last.ok {
            note_landed("create_dvp_span");
            self.dvps.retain(|d| d.swap_dvp != swap_dvp);
            if self.dvps.len() < 8 {
                self.dvps.push(DvpRecord {
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
                    amount_a: 1_000,
                    amount_b: 2_000,
                    token_program_a: token_program,
                    token_program_b: token_program,
                });
            }
        } else {
            note_blocked(
                "create_dvp_span",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
        }
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        self.last.ok
    }
    //added:mubariz
    #[allow(non_snake_case)]
    pub fn action_MODEL_reject_wrong_token_program(
        &mut self,
        which_dvp: u64,
        which_party: u64,
    ) -> bool {
        if self.dvps.is_empty() {
            note_blocked("reject_wrong_token_program", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let party = if which_party % 2 == 0 {
            d.user_a
        } else {
            d.user_b
        };
        let signer = match self.actors.iter().find(|k| k.pubkey() == party) {
            Some(k) => k.clone(),
            None => {
                note_blocked("reject_wrong_token_program", -1, &"party keypair not found");
                return false;
            }
        };
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        // flip token program — SPL DVP gets T22, T22 DVP gets SPL
        let wrong_program = if d.token_program_a == Self::TOKEN_PROGRAM {
            Self::TOKEN_PROGRAM_2022
        } else {
            Self::TOKEN_PROGRAM
        };
        self.snapshot();
        self.last = Action::new("reject_wrong_token_program", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_reject_dvp_ix(
                self.program_id,
                party,
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_a,
                ub_b,
                wrong_program,
                wrong_program,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = false;
        note_blocked(
            "reject_wrong_token_program",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }
    //added:mubariz
    #[allow(non_snake_case)]
    pub fn action_MODEL_reject_wrong_destination(
        &mut self,
        which_dvp: u64,
        which_party: u64,
    ) -> bool {
        if self.dvps.is_empty() {
            note_blocked("reject_wrong_destination", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let party = if which_party % 2 == 0 {
            d.user_a
        } else {
            d.user_b
        };
        let signer = match self.actors.iter().find(|k| k.pubkey() == party) {
            Some(k) => k.clone(),
            None => {
                note_blocked("reject_wrong_destination", -1, &"party keypair not found");
                return false;
            }
        };
        // swap destinations — user_b's ATA where user_a's expected and vice versa
        let wrong_ua_a = Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_a);
        let wrong_ub_b = Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_b);
        self.snapshot();
        self.last = Action::new("reject_wrong_destination", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_reject_dvp_ix(
                self.program_id,
                party,
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                wrong_ua_a,
                wrong_ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = false;
        note_blocked(
            "reject_wrong_destination",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }
    // added:mubariz->targeted negative test designed to verify that the program correctly rejects reject_dvp when mint arguments are intentionally swapped. By passing mint_b as mint_a (and vice versa) along with mismatched dst ATAs
    #[allow(non_snake_case)]
    pub fn action_MODEL_reject_swapped_mints(&mut self, which_dvp: u64, which_party: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("reject_swapped_mints", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        let party = if which_party % 2 == 0 {
            d.user_a
        } else {
            d.user_b
        };
        let signer = match self.actors.iter().find(|k| k.pubkey() == party) {
            Some(k) => k.clone(),
            None => {
                note_blocked("reject_swapped_mints", -1, &"party keypair not found");
                return false;
            }
        };
        let ua_b = Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_a);
        let ub_a = Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_b);
        self.snapshot();
        self.last = Action::new("reject_swapped_mints", vec![]);
        let outcome = self
            .ctx
            .raw_call(build_reject_dvp_ix(
                self.program_id,
                party,
                d.swap_dvp,
                d.mint_b,
                d.mint_a,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_b,
                ub_a,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = false;
        note_blocked(
            "reject_swapped_mints",
            match outcome.as_ref() {
                Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                Err(_) => -1,
            },
            &outcome,
        );
        false
    }
    //upgraded the harness with Token-2022 support by using derive_ata_with_program and passing dynamic token program IDs to ix,
    //  also,creating ATAs only for standard SPL Token to avoid conflicts.preserves closed DVP records (by commenting out dvps.retain()),
    // TODO:dvp retain or what ?
    #[allow(non_snake_case)]
    pub fn action_MODEL_close_dvp_for_recover(
        &mut self,
        which_dvp: u64,
        #[range(0..1_000)] preload: u64,
    ) -> bool {
        if self.dvps.is_empty() {
            note_blocked("close_dvp_for_recover", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        // Guard: swap_dvp must still be program-owned.
        if !self
            .ctx
            .get_account(&d.swap_dvp)
            .map(|a| !a.data.is_empty())
            .unwrap_or(false)
        {
            note_blocked("close_dvp_for_recover", -1, &"swap_dvp already closed");
            return false;
        }
        let signer = match self.actors.iter().find(|k| k.pubkey() == d.user_a) {
            Some(k) => k.clone(),
            None => {
                note_blocked("close_dvp_for_recover", -1, &"user_a keypair not found");
                return false;
            }
        };
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        // Refund destinations must exist if a leg is funded, or the refund CPI
        // reverts the whole reject.
        // CHANGE the ATA creation loop in close_dvp_for_recover:
        for (owner, mint, ata) in [(d.user_a, d.mint_a, ua_a), (d.user_b, d.mint_b, ub_b)] {
            if !self
                .ctx
                .get_account(&ata)
                .map(|a| a.data.len() >= 72)
                .unwrap_or(false)
            {
                if d.token_program_a == Self::TOKEN_PROGRAM {
                    // Only create SPL Token ATAs inline , T22 ATAs created in setup
                    let _ = self
                        .ctx
                        .create_token_account()
                        .pubkey(ata)
                        .mint(mint)
                        .token_owner(owner)
                        .amount(0)
                        .create();
                }
            }
        }
        for k in [
            d.swap_dvp,
            d.tombstone,
            d.dvp_ata_a,
            d.dvp_ata_b,
            ua_a,
            ub_b,
        ] {
            self.watch(k);
        }
        self.snapshot();
        self.last = Action::new("reject_dvp", vec![("leg_a_extras_count", 0u128)]);
        self.last.accounts = vec![
            ("signer", d.user_a),
            ("swap_dvp", d.swap_dvp),
            ("mint_a", d.mint_a),
            ("mint_b", d.mint_b),
            ("dvp_ata_a", d.dvp_ata_a),
            ("dvp_ata_b", d.dvp_ata_b),
            ("user_a_ata_a", ua_a),
            ("user_b_ata_b", ub_b),
            ("token_program_a", d.token_program_a),
            ("token_program_b", d.token_program_b),
            ("memo_program", Self::MEMO_PROGRAM),
        ];
        let outcome = self
            .ctx
            .raw_call(build_reject_dvp_ix(
                self.program_id,
                d.user_a,
                d.swap_dvp,
                d.mint_a,
                d.mint_b,
                d.dvp_ata_a,
                d.dvp_ata_b,
                ua_a,
                ub_b,
                d.token_program_a,
                d.token_program_b,
                Self::MEMO_PROGRAM,
                0,
            ))
            .signers(&[&*signer])
            .send();
        self.last.ok = outcome.as_ref().map(|o| o.is_success()).unwrap_or(false);
        self.last.fee = outcome.as_ref().map(|o| o.fee()).unwrap_or(0);
        if !self.last.ok {
            note_blocked(
                "close_dvp_for_recover",
                match outcome.as_ref() {
                    Ok(o) => o.error_code().map(|c| c as i64).unwrap_or(-2),
                    Err(_) => -1,
                },
                &outcome,
            );
            return false;
        }
        note_landed("close_dvp_for_recover");
        // The SwapDvp is now system-owned and empty; the tombstone survives.
        self.dvps.retain(|x| x.swap_dvp != d.swap_dvp);
        if !self.closed.iter().any(|x| x.swap_dvp == d.swap_dvp) && self.closed.len() < 8 {
            self.closed.push(d.clone());
        }
        // Recreate BOTH escrow ATAs for the dead PDA, as a late deposit would:
        // reject closed them, so they no longer exist. Owner is the (now
        // system-owned) swap_dvp PDA. Use the DVP's actual token program per
        // leg so recover_real can match the escrow's account owner correctly.
        for (mint, escrow, token_prog) in [
            (d.mint_a, d.dvp_ata_a, d.token_program_a),
            (d.mint_b, d.dvp_ata_b, d.token_program_b),
        ] {
            if !self
                .ctx
                .get_account(&escrow)
                .map(|a| a.data.len() >= 72)
                .unwrap_or(false)
            {
                if token_prog == Self::TOKEN_PROGRAM_2022 {
                    let mut data = vec![0u8; 165];
                    data[0..32].copy_from_slice(&mint.to_bytes());
                    data[32..64].copy_from_slice(&d.swap_dvp.to_bytes());
                    data[64..72].copy_from_slice(&preload.to_le_bytes());
                    data[108] = 1; // is_initialized
                    let account = solana_account::Account {
                        lamports: 2_039_280,
                        data,
                        owner: Self::TOKEN_PROGRAM_2022,
                        executable: false,
                        rent_epoch: u64::MAX,
                    };
                    let _ = self.ctx.svm.set_account(escrow, account.into());
                } else {
                    let _ = self
                        .ctx
                        .create_token_account()
                        .pubkey(escrow)
                        .mint(mint)
                        .token_owner(d.swap_dvp)
                        .amount(preload)
                        .create();
                }
            }
            self.watch(escrow);
        }
        true
    }
    #[allow(non_snake_case)]
    pub fn action_MODEL_fund_legs_tx(&mut self, which_dvp: u64, #[range(0..8)] extra: u64) -> bool {
        if self.dvps.is_empty() {
            note_blocked("fund_legs_tx", -1, &"no dvps yet");
            return false;
        }
        let d = self.dvps[(which_dvp as usize) % self.dvps.len()].clone();
        // mint authority is actor 0 (see action_MODEL_create_mint_pair)
        let authority = match self.actor(0) {
            Some(a) => a,
            None => {
                note_blocked("fund_legs_tx", -1, &"no mint authority (actor 0)");
                return false;
            }
        };
        let user_a_kp = match self.actors.iter().find(|k| k.pubkey() == d.user_a) {
            Some(k) => k.clone(),
            None => {
                note_blocked("fund_legs_tx", -1, &"user_a keypair not found");
                return false;
            }
        };
        let user_b_kp = match self.actors.iter().find(|k| k.pubkey() == d.user_b) {
            Some(k) => k.clone(),
            None => {
                note_blocked("fund_legs_tx", -1, &"user_b keypair not found");
                return false;
            }
        };

        // Every ATA both legs and both close paths can touch.
        let ua_a = Self::derive_ata_with_program(&d.user_a, &d.mint_a, &d.token_program_a);
        let ua_b = Self::derive_ata_with_program(&d.user_a, &d.mint_b, &d.token_program_b);
        let ub_a = Self::derive_ata_with_program(&d.user_b, &d.mint_a, &d.token_program_a);
        let ub_b = Self::derive_ata_with_program(&d.user_b, &d.mint_b, &d.token_program_b);
        for (owner, mint, ata) in [
            (d.user_a, d.mint_a, ua_a),
            (d.user_a, d.mint_b, ua_b),
            (d.user_b, d.mint_a, ub_a),
            (d.user_b, d.mint_b, ub_b),
        ] {
            if !self
                .ctx
                .get_account(&ata)
                .map(|a| a.data.len() >= 72)
                .unwrap_or(false)
            {
                let _ = self
                    .ctx
                    .create_token_account()
                    .pubkey(ata)
                    .mint(mint)
                    .token_owner(owner)
                    .amount(0)
                    .create();
            }
            self.watch(ata);
        }

        let amt_a = d.amount_a.saturating_add(extra % 3);
        let amt_b = d.amount_b.saturating_add(extra % 5);

        self.snapshot();
        self.last = Action::new(
            "model_fund_legs_tx",
            vec![
                ("extra", extra as u128),
                ("amount_a", amt_a as u128),
                ("amount_b", amt_b as u128),
            ],
        );
        self.last.accounts = vec![
            ("swap_dvp", d.swap_dvp),
            ("dvp_ata_a", d.dvp_ata_a),
            ("dvp_ata_b", d.dvp_ata_b),
            ("user_a_ata_a", ua_a),
            ("user_b_ata_b", ub_b),
        ];
        for k in [d.dvp_ata_a, d.dvp_ata_b] {
            self.watch(k);
        }

        let mut ok = true;
        // 1) MintTo each party's own-leg ATA, signed by the mint authority.
        for (mint, dest, amount) in [(d.mint_a, ua_a, amt_a), (d.mint_b, ub_b, amt_b)] {
            let mut data: Vec<u8> = vec![7]; // SPL Token MintTo
            data.extend_from_slice(&amount.to_le_bytes());
            let ix = Instruction {
                program_id: Self::TOKEN_PROGRAM,
                accounts: vec![
                    AccountMeta::new(mint, false),
                    AccountMeta::new(dest, false),
                    AccountMeta::new_readonly(authority.pubkey(), true),
                ],
                data,
            };
            let r = self.ctx.raw_call(ix).signers(&[&*authority]).send();
            ok &= r.as_ref().map(|o| o.is_success()).unwrap_or(false);
        }

        // 2) Transfer from each party's ATA into the leg escrow — the
        //    documented funding path, a plain SPL Transfer.
        for (src, dst, owner_kp, amount) in [
            (ua_a, d.dvp_ata_a, user_a_kp.clone(), amt_a),
            (ub_b, d.dvp_ata_b, user_b_kp.clone(), amt_b),
        ] {
            let mut data: Vec<u8> = vec![3]; // SPL Token Transfer
            data.extend_from_slice(&amount.to_le_bytes());
            let ix = Instruction {
                program_id: Self::TOKEN_PROGRAM,
                accounts: vec![
                    AccountMeta::new(src, false),
                    AccountMeta::new(dst, false),
                    AccountMeta::new_readonly(owner_kp.pubkey(), true),
                ],
                data,
            };
            let r = self.ctx.raw_call(ix).signers(&[&*owner_kp]).send();
            ok &= r.as_ref().map(|o| o.is_success()).unwrap_or(false);
        }
        self.last.ok = ok;
        if ok {
            note_landed("fund_legs_tx");
        } else {
            note_blocked("fund_legs_tx", -2, &"MintTo or Transfer CPI failed");
        }
        ok
    }
}
// Invariants are spliced in above, each taking `fixture: &mut DvpSwapProgramFixture`.
#[invariant_test]
fn invariant_test(fixture: &mut DvpSwapProgramFixture) {
    let _ = fixture;
}
