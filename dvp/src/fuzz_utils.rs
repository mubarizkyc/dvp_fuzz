use solana_pubkey::Pubkey;
use std::collections::HashSet;

#[derive(Clone)]
pub struct DvpRecord {
    pub swap_dvp: Pubkey,
    pub tombstone: Pubkey,
    pub authority: Pubkey,
    pub user_a: Pubkey,
    pub user_b: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub dvp_ata_a: Pubkey,
    pub dvp_ata_b: Pubkey,
    pub nonce: u64,
    pub amount_a: u64,
    pub amount_b: u64,
    pub token_program_a: Pubkey,
    pub token_program_b: Pubkey,
}

#[derive(Clone, Default)]
pub struct Action {
    pub name: &'static str,
    pub args: Vec<(&'static str, u128)>,
    // WHICH accounts this action used, by the name the IDL gives them.
    pub accounts: Vec<(&'static str, Pubkey)>,
    pub ok: bool,
    pub fee: u64,
}

impl Action {
    pub fn new(name: &'static str, args: Vec<(&'static str, u128)>) -> Self {
        Self {
            name,
            args,
            accounts: Vec::new(),
            ok: false,
            fee: 0,
        }
    }
    /// The account this action passed under that name, if it passed one.
    pub fn account(&self, name: &str) -> Option<Pubkey> {
        self.accounts
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, k)| *k)
    }
    /// True when this action is `name` and it succeeded.
    pub fn was(&self, name: &str) -> bool {
        self.name == name && self.ok
    }
    /// An argument it was called with, or 0 if this action has no such argument.
    pub fn arg(&self, name: &str) -> u128 {
        self.args
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| *v)
            .unwrap_or(0)
    }
}

thread_local! {
    static BLOCKED_SEEN: std::cell::RefCell<HashSet<(&'static str, i64)>> =
        std::cell::RefCell::new(HashSet::new());
    static LANDED_SEEN: std::cell::RefCell<HashSet<&'static str>> =
        std::cell::RefCell::new(HashSet::new());
}

pub fn note_landed(action: &'static str) {
    LANDED_SEEN.with(|seen| {
        if seen.borrow_mut().insert(action) {
            eprintln!("[LANDED] {}", action);
        }
    });
}

/// Say what blocked an action, the first time this thread sees that pair.
///
/// -1 means it never reached the SVM (a signature the fixture could not produce);
/// -2 means it failed with no Anchor code, which is a runtime error rather than a
/// guard - a missing account, usually.
pub fn note_blocked(action: &'static str, code: i64, detail: &dyn std::fmt::Debug) {
    BLOCKED_SEEN.with(|seen| {
        if seen.borrow_mut().insert((action, code)) {
            match code {
                -1 => eprintln!(
                    "[BLOCKED] {} -> not submitted\n  detail: {:?}",
                    action, detail
                ),
                -2 => eprintln!(
                    "[BLOCKED] {} -> failed with no error code\n  detail: {:?}",
                    action, detail
                ),
                c if c >= 6000 => eprintln!(
                    "[BLOCKED] {} -> Custom({}) = error enum variant #{}\n  detail: {:?}",
                    action,
                    c,
                    c - 6000,
                    detail
                ),
                c => eprintln!(
                    "[BLOCKED] {} -> Custom({})\n  detail: {:?}",
                    action, c, detail
                ),
            }
        }
    });
}
