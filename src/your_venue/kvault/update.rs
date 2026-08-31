use solana_account::Account;
use solana_pubkey::Pubkey;

use super::KVAULT_PROGRAM_ID;
use super::state::{self, KvaultState, GLOBAL_CONFIG_SEED, PD_OFF_UNDERLYING_TOKEN_KIND};
use crate::account_caching::AccountsCache;
use crate::your_venue::OVERPASS_PROGRAM_ID;
use crate::your_venue::common::bytes::{read_pubkey, read_u64, read_u128};
use crate::your_venue::common::programs::{
    derive_ata, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use super::math;
use crate::your_venue::klend;
use crate::your_venue::state::WrapperVault;
use crate::trading_venue::error::TradingVenueError;

pub const CLOCK_SYSVAR_ID: Pubkey =
    Pubkey::from_str_const("SysvarC1ock11111111111111111111111111111111");
const FARMS_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");
const FARMS_USER_ACTIVE_STAKE_SCALED_OFFSET: usize = 408;
const FARMS_USER_STATE_LEN: usize = 920;
const SPL_TOKEN_AMOUNT_OFFSET: usize = 64;
const MAX_QUOTE_DEPOSIT: u64 = u64::MAX / 32;

pub fn required_pubkeys(state: &KvaultState, wv: &WrapperVault) -> Vec<Pubkey> {
    let (kvault_global_config, _) =
        Pubkey::find_program_address(&[GLOBAL_CONFIG_SEED], &KVAULT_PROGRAM_ID);
    let mut accts = vec![wv.source_pool, kvault_global_config, CLOCK_SYSVAR_ID];
    for alloc in &state.active_allocations {
        accts.push(alloc.reserve);
    }
    if wv.protocol_data[48] != 0 {
        accts.push(wv.source_position_pda);
    }
    accts
}

pub async fn run(
    state: &mut KvaultState,
    current_slot: &mut u64,
    unix_timestamp: &mut i64,
    wv: &WrapperVault,
    cache: &dyn AccountsCache,
) -> Result<(), TradingVenueError> {
    let vault_acct = cache
        .get_account(&wv.source_pool)
        .await?
        .ok_or(TradingVenueError::NoAccountFound(wv.source_pool.into()))?;
    let mut new_state = state::decode(&vault_acct.data)?;

    new_state.global_withdrawal_penalty_lamports = state.global_withdrawal_penalty_lamports;
    new_state.global_withdrawal_penalty_bps = state.global_withdrawal_penalty_bps;

    let (kvault_global_config, _) =
        Pubkey::find_program_address(&[GLOBAL_CONFIG_SEED], &KVAULT_PROGRAM_ID);
    let mut to_fetch: Vec<Pubkey> = vec![kvault_global_config, CLOCK_SYSVAR_ID];
    for alloc in &new_state.active_allocations {
        to_fetch.push(alloc.reserve);
    }
    if wv.protocol_data[48] != 0 {
        to_fetch.push(wv.source_position_pda);
    }

    let fetched: Vec<Option<Account>> = cache.get_accounts(&to_fetch).await?;
    let mut iter = fetched.into_iter();

    let gc_opt = iter.next().flatten();
    let clock_opt = iter.next().flatten();

    if let Some(gc_acct) = gc_opt {
        let (lamports, bps) = state::decode_global_config_penalty(&gc_acct.data)?;
        new_state.global_withdrawal_penalty_lamports = lamports;
        new_state.global_withdrawal_penalty_bps = bps;
    }

    let clock_acct =
        clock_opt.ok_or(TradingVenueError::NoAccountFound(CLOCK_SYSVAR_ID.into()))?;
    let slot_bytes: [u8; 8] = clock_acct
        .data
        .get(0..8)
        .ok_or(TradingVenueError::DeserializationError("clock.slot".into()))?
        .try_into()
        .map_err(|_| TradingVenueError::DeserializationError("clock.slot".into()))?;
    *current_slot = u64::from_le_bytes(slot_bytes);
    let ts_bytes: [u8; 8] = clock_acct
        .data
        .get(32..40)
        .ok_or(TradingVenueError::DeserializationError(
            "clock.unix_timestamp".into(),
        ))?
        .try_into()
        .map_err(|_| TradingVenueError::DeserializationError("clock.unix_timestamp".into()))?;
    *unix_timestamp = i64::from_le_bytes(ts_bytes);

    let mut cached_reserves = Vec::with_capacity(new_state.active_allocations.len());
    for alloc in &new_state.active_allocations {
        if let Some(reserve_acct) = iter.next().flatten() {
            let decoded = klend::state::decode(&reserve_acct.data)?;
            cached_reserves.push((alloc.reserve, decoded));
        } else if let Some((_, prev)) = state
            .cached_reserves
            .iter()
            .find(|(k, _)| *k == alloc.reserve)
        {
            cached_reserves.push((alloc.reserve, prev.clone()));
        }
    }
    new_state.cached_reserves = cached_reserves;
    for (_, reserve) in new_state.cached_reserves.iter_mut() {
        klend::math::pre_accrue(reserve, *current_slot, *unix_timestamp)?;
    }

    if wv.protocol_data[48] != 0 {
        if let Some(fs_acct) = iter.next().flatten() {
            new_state.cached_farm_state = Some(state::decode_farm_state_minimal(&fs_acct.data)?);
        } else {
            new_state.cached_farm_state = state.cached_farm_state;
        }
    }

    new_state.position_kvault_shares = if wv.protocol_data[48] != 0 {
        let (vault_authority, _) = Pubkey::find_program_address(
            &[b"authority", wv.wrapper_mint.as_ref()],
            &OVERPASS_PROGRAM_ID,
        );
        let kvault_share_mint =
            read_pubkey(&wv.protocol_data, 0, "kvault.pd.share_mint")?;
        let vault_shares_ata =
            derive_ata(&vault_authority, &kvault_share_mint, &SPL_TOKEN_PROGRAM_ID);
        let (farms_user_state, _) = Pubkey::find_program_address(
            &[b"user", wv.source_position_pda.as_ref(), vault_authority.as_ref()],
            &FARMS_PROGRAM_ID,
        );
        let accts = cache
            .get_accounts(&[vault_shares_ata, farms_user_state])
            .await?;
        let mut it = accts.into_iter();
        let idle = match it.next().flatten() {
            Some(a) => read_u64(&a.data, SPL_TOKEN_AMOUNT_OFFSET, "shares_ata.amount")?,
            None => 0,
        };
        let user_stake_scaled = match it.next().flatten() {
            Some(a)
                if a.owner == FARMS_PROGRAM_ID
                    && a.data.len() >= FARMS_USER_STATE_LEN =>
            {
                read_u128(
                    &a.data,
                    FARMS_USER_ACTIVE_STAKE_SCALED_OFFSET,
                    "farms_user.active_stake_scaled",
                )?
            }
            _ => 0,
        };
        let fm = new_state.cached_farm_state.ok_or(TradingVenueError::MathError(
            "kvault: use_farms but FarmState not cached".into(),
        ))?;
        let farm_kt = state::convert_stake_to_amount(
            user_stake_scaled,
            fm.total_active_stake_scaled,
            fm.total_staked_amount,
        )?;
        (idle as u128)
            .checked_add(farm_kt as u128)
            .ok_or(TradingVenueError::MathError("position sum".into()))?
    } else {
        wv.intermediate_held
    };

    new_state.underlying_token_program = if wv.protocol_data[PD_OFF_UNDERLYING_TOKEN_KIND] == 1 {
        TOKEN_2022_PROGRAM_ID
    } else {
        SPL_TOKEN_PROGRAM_ID
    };

    let aum = math::compute_current_aum(&new_state, *current_slot, *unix_timestamp).ok();
    new_state.cached_aum_sf = aum.map(|f| f.to_bits());

    let protocol_cap: u64 = if new_state.deposit_cap == 0 {
        u64::MAX
    } else {
        let aum_ceil: u64 = aum
            .map(|f| f.ceil().to_num::<u128>().min(u64::MAX as u128) as u64)
            .unwrap_or(u64::MAX);
        new_state.deposit_cap.saturating_sub(aum_ceil)
    };
    new_state.cached_max_deposit = protocol_cap.min(MAX_QUOTE_DEPOSIT);

    *state = new_state;
    Ok(())
}
