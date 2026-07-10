use solana_account::Account;
use solana_pubkey::Pubkey;

use super::math;
use super::state::{self, MarginfiState, PD_OFF_UNDERLYING_TOKEN_KIND};
use crate::account_caching::AccountsCache;
use crate::your_venue::common::bytes::read_pubkey;
use crate::your_venue::common::programs::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
use crate::your_venue::state::WrapperVault;
use crate::trading_venue::error::TradingVenueError;

pub const CLOCK_SYSVAR_ID: Pubkey =
    Pubkey::from_str_const("SysvarC1ock11111111111111111111111111111111");

const PD_OFF_GROUP: usize = 0;

pub fn required_pubkeys(wv: &WrapperVault) -> Vec<Pubkey> {
    let group = read_pubkey(&wv.protocol_data, PD_OFF_GROUP, "marginfi.protocol_data.group")
        .unwrap_or_default();
    vec![
        wv.source_pool,
        wv.source_position_pda,
        group,
        CLOCK_SYSVAR_ID,
    ]
}

pub async fn run(
    s: &mut MarginfiState,
    unix_timestamp: &mut i64,
    wv: &WrapperVault,
    cache: &dyn AccountsCache,
) -> Result<(), TradingVenueError> {
    let group_pk = read_pubkey(&wv.protocol_data, PD_OFF_GROUP, "marginfi.protocol_data.group")?;
    let accounts = cache
        .get_accounts(&[
            wv.source_pool,
            wv.source_position_pda,
            group_pk,
            CLOCK_SYSVAR_ID,
        ])
        .await?;
    let [bank_opt, position_opt, group_opt, clock_opt]: [Option<Account>; 4] = accounts
        .try_into()
        .map_err(|_| TradingVenueError::FailedToFetchMultipleAccountData)?;

    let bank_acct = bank_opt.ok_or(TradingVenueError::NoAccountFound(wv.source_pool.into()))?;
    let mut bank = state::decode(&bank_acct.data)?;

    if let Some(acct) = position_opt {
        bank.cached_user_asset_shares =
            state::read_asset_shares_for_bank(&acct.data, &wv.source_pool)?;
    }

    let group_acct = group_opt.ok_or(TradingVenueError::NoAccountFound(group_pk.into()))?;
    let group = state::decode_group(&group_acct.data)?;
    bank.group_program_fees_enabled = group.program_fees_enabled;
    bank.group_program_fee_fixed = group.program_fee_fixed;
    bank.group_program_fee_rate = group.program_fee_rate;

    bank.underlying_token_program = if wv.protocol_data[PD_OFF_UNDERLYING_TOKEN_KIND] == 1 {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };

    let clock_acct =
        clock_opt.ok_or(TradingVenueError::NoAccountFound(CLOCK_SYSVAR_ID.into()))?;
    let ts_bytes: [u8; 8] = clock_acct
        .data
        .get(32..40)
        .ok_or(TradingVenueError::DeserializationError(
            "clock.unix_timestamp".into(),
        ))?
        .try_into()
        .map_err(|_| TradingVenueError::DeserializationError("clock.unix_timestamp".into()))?;
    *unix_timestamp = i64::from_le_bytes(ts_bytes);

    *s = bank;
    math::pre_accrue(s, *unix_timestamp)?;
    Ok(())
}
