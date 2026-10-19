use solana_pubkey::Pubkey;

use super::bytes::read_u64;
use crate::account_caching::AccountsCache;
use crate::trading_venue::error::TradingVenueError;

const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;

pub const SPL_TOKEN_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const ATA_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const SYSTEM_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("11111111111111111111111111111111");

pub fn derive_ata(owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ATA_PROGRAM_ID,
    )
    .0
}

pub async fn read_token_amount_or_zero(
    cache: &dyn AccountsCache,
    key: &Pubkey,
) -> Result<u64, TradingVenueError> {
    match cache.get_account(key).await? {
        Some(a) if !a.data.is_empty() => {
            read_u64(&a.data, TOKEN_ACCOUNT_AMOUNT_OFFSET, "token_account.amount")
        }
        _ => Ok(0),
    }
}
