use solana_account::Account;
use solana_pubkey::Pubkey;

use super::math;
use super::state::{self, SaveState};
use crate::account_caching::AccountsCache;
use crate::your_venue::state::WrapperVault;
use crate::trading_venue::error::TradingVenueError;

pub const CLOCK_SYSVAR_ID: Pubkey =
    Pubkey::from_str_const("SysvarC1ock11111111111111111111111111111111");

pub fn required_pubkeys(wv: &WrapperVault) -> Vec<Pubkey> {
    vec![wv.source_pool, CLOCK_SYSVAR_ID]
}

pub async fn run(
    s: &mut SaveState,
    current_slot: &mut u64,
    wv: &WrapperVault,
    cache: &dyn AccountsCache,
) -> Result<(), TradingVenueError> {
    let accounts = cache
        .get_accounts(&[wv.source_pool, CLOCK_SYSVAR_ID])
        .await?;
    let [reserve_opt, clock_opt]: [Option<Account>; 2] = accounts
        .try_into()
        .map_err(|_| TradingVenueError::FailedToFetchMultipleAccountData)?;
    let reserve_acct = reserve_opt
        .ok_or(TradingVenueError::NoAccountFound(wv.source_pool.into()))?;
    let clock_acct =
        clock_opt.ok_or(TradingVenueError::NoAccountFound(CLOCK_SYSVAR_ID.into()))?;

    *s = state::decode(&reserve_acct.data)?;
    let slot_bytes: [u8; 8] = clock_acct
        .data
        .get(0..8)
        .ok_or(TradingVenueError::DeserializationError("clock.slot".into()))?
        .try_into()
        .map_err(|_| TradingVenueError::DeserializationError("clock.slot".into()))?;
    *current_slot = u64::from_le_bytes(slot_bytes);
    math::pre_accrue(s, *current_slot)?;
    Ok(())
}
