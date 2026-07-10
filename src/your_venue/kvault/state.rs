use solana_pubkey::Pubkey;

use crate::your_venue::common::bytes::{read_pubkey, read_u128, read_u64};
use crate::your_venue::common::programs::SPL_TOKEN_PROGRAM_ID;
use crate::your_venue::common::u256::U256;
use crate::your_venue::klend::fraction::Fraction;
use crate::your_venue::klend::state::KlendState;
use crate::trading_venue::error::TradingVenueError;

pub const VAULT_STATE_LEN: usize = 8 + 62544;
pub const MAX_RESERVES: usize = 25;
pub const VAULT_ALLOCATION_SIZE: usize = 2160;
pub const VAULT_STATE_DISCRIMINATOR: [u8; 8] = [228, 196, 82, 165, 98, 210, 235, 152];

pub const PD_OFF_UNDERLYING_TOKEN_KIND: usize = 49;

const OFF_TOKEN_MINT: usize = 8 + 72;
const OFF_TOKEN_MINT_DECIMALS: usize = 8 + 104;
const OFF_TOKEN_VAULT: usize = 8 + 112;
const OFF_SHARES_MINT: usize = 8 + 176;
const OFF_TOKEN_AVAILABLE: usize = 8 + 216;
const OFF_SHARES_ISSUED: usize = 8 + 224;
const OFF_CRANK_FUND_FEE: usize = 8 + 58432;
const OFF_PERFORMANCE_FEE_BPS: usize = 8 + 248;
const OFF_MANAGEMENT_FEE_BPS: usize = 8 + 256;
const OFF_LAST_FEE_CHARGE_TIMESTAMP: usize = 8 + 264;
const OFF_PREV_AUM_SF: usize = 8 + 272;
const OFF_PENDING_FEES_SF: usize = 8 + 288;
const OFF_VAULT_ALLOCATION_STRATEGY: usize = 8 + 304;
const OFF_MIN_DEPOSIT_AMOUNT: usize = 8 + 58400;
const OFF_MIN_WITHDRAW_AMOUNT: usize = 8 + 58408;
const OFF_BASE_VAULT_AUTHORITY: usize = 8 + 32;
const OFF_WITHDRAWAL_PENALTY_LAMPORTS: usize = 8 + 58672;
const OFF_WITHDRAWAL_PENALTY_BPS: usize = 8 + 58680;

const ALLOC_OFF_RESERVE: usize = 0;
const ALLOC_OFF_CTOKEN_VAULT: usize = 32;
const ALLOC_OFF_TARGET_ALLOCATION_WEIGHT: usize = 64;
const ALLOC_OFF_TOKEN_ALLOCATION_CAP: usize = 72;
const ALLOC_OFF_CTOKEN_ALLOCATION: usize = 1104;

pub const GLOBAL_CONFIG_LEN: usize = 8 + 1024;
pub const GLOBAL_CONFIG_SEED: &[u8] = b"global_config";

const KGC_OFF_WITHDRAWAL_PENALTY_LAMPORTS: usize = 8 + 64;
const KGC_OFF_WITHDRAWAL_PENALTY_BPS: usize = 8 + 72;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct ActiveAllocation {
    pub reserve: Pubkey,
    pub ctoken_vault: Pubkey,
    pub target_allocation_weight: u64,
    pub token_allocation_cap: u64,
    pub ctoken_allocation: u64,
    pub slot_index: usize,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct KvaultState {
    pub token_mint: Pubkey,
    pub token_mint_decimals: u8,
    pub token_vault: Pubkey,
    pub shares_mint: Pubkey,
    pub base_vault_authority: Pubkey,
    pub token_available: u64,
    pub shares_issued: u64,
    pub crank_fund_fee_per_reserve: u64,
    pub performance_fee_bps: u64,
    pub management_fee_bps: u64,
    pub last_fee_charge_timestamp: u64,
    pub prev_aum_sf: u128,
    pub pending_fees_sf: u128,
    pub min_deposit_amount: u64,
    pub min_withdraw_amount: u64,
    pub vault_withdrawal_penalty_lamports: u64,
    pub vault_withdrawal_penalty_bps: u64,
    pub active_allocations: Vec<ActiveAllocation>,
    pub global_withdrawal_penalty_lamports: u64,
    pub global_withdrawal_penalty_bps: u64,
    pub cached_reserves: Vec<(Pubkey, KlendState)>,
    pub cached_farm_state: Option<FarmStateMinimal>,
    pub underlying_token_program: Pubkey,
    pub position_kvault_shares: u128,
    pub cached_aum_sf: Option<u128>,
    pub cached_max_deposit: u64,
}

impl Default for KvaultState {
    fn default() -> Self {
        Self {
            token_mint: Pubkey::default(),
            token_mint_decimals: 0,
            token_vault: Pubkey::default(),
            shares_mint: Pubkey::default(),
            base_vault_authority: Pubkey::default(),
            token_available: 0,
            shares_issued: 0,
            crank_fund_fee_per_reserve: 0,
            performance_fee_bps: 0,
            management_fee_bps: 0,
            last_fee_charge_timestamp: 0,
            prev_aum_sf: 0,
            pending_fees_sf: 0,
            min_deposit_amount: 0,
            min_withdraw_amount: 0,
            vault_withdrawal_penalty_lamports: 0,
            vault_withdrawal_penalty_bps: 0,
            active_allocations: Vec::new(),
            global_withdrawal_penalty_lamports: 0,
            global_withdrawal_penalty_bps: 0,
            cached_reserves: Vec::new(),
            cached_farm_state: None,
            underlying_token_program: SPL_TOKEN_PROGRAM_ID,
            position_kvault_shares: 0,
            cached_aum_sf: None,
            cached_max_deposit: u64::MAX,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FarmStateMinimal {
    pub farm_vault: Pubkey,
    pub farm_vaults_authority: Pubkey,
    pub scope_prices: Option<Pubkey>,
    pub total_active_stake_scaled: u128,
    pub total_staked_amount: u64,
}

const FARM_OFF_VAULT: usize = 7256;
const FARM_OFF_VAULTS_AUTHORITY: usize = 7288;
const FARM_OFF_SCOPE_PRICES: usize = 7536;
const FARM_MIN_LEN: usize = FARM_OFF_SCOPE_PRICES + 32;
const FARM_OFF_TOTAL_STAKED_AMOUNT: usize = 7248;
const FARM_OFF_TOTAL_ACTIVE_STAKE_SCALED: usize = 7408;

pub fn decode(data: &[u8]) -> Result<KvaultState, TradingVenueError> {
    if data.len() < VAULT_STATE_LEN {
        return Err(TradingVenueError::DeserializationError(
            "kvault state: length".into(),
        ));
    }
    if data[..8] != VAULT_STATE_DISCRIMINATOR {
        return Err(TradingVenueError::DeserializationError(
            "kvault state: discriminator".into(),
        ));
    }

    let token_mint = read_pubkey(data, OFF_TOKEN_MINT, "kvault.token_mint")?;
    let token_mint_decimals =
        read_u64(data, OFF_TOKEN_MINT_DECIMALS, "kvault.token_mint_decimals")? as u8;
    let token_vault = read_pubkey(data, OFF_TOKEN_VAULT, "kvault.token_vault")?;
    let shares_mint = read_pubkey(data, OFF_SHARES_MINT, "kvault.shares_mint")?;
    let base_vault_authority = read_pubkey(data, OFF_BASE_VAULT_AUTHORITY, "kvault.base_vault_authority")?;
    let token_available = read_u64(data, OFF_TOKEN_AVAILABLE, "kvault.token_available")?;
    let shares_issued = read_u64(data, OFF_SHARES_ISSUED, "kvault.shares_issued")?;
    let crank_fund_fee_per_reserve = read_u64(data, OFF_CRANK_FUND_FEE, "kvault.crank_fund_fee")?;
    let performance_fee_bps = read_u64(data, OFF_PERFORMANCE_FEE_BPS, "kvault.performance_fee_bps")?;
    let management_fee_bps = read_u64(data, OFF_MANAGEMENT_FEE_BPS, "kvault.management_fee_bps")?;
    let last_fee_charge_timestamp =
        read_u64(data, OFF_LAST_FEE_CHARGE_TIMESTAMP, "kvault.last_fee_charge_timestamp")?;
    let prev_aum_sf = read_u128(data, OFF_PREV_AUM_SF, "kvault.prev_aum_sf")?;
    let pending_fees_sf = read_u128(data, OFF_PENDING_FEES_SF, "kvault.pending_fees_sf")?;
    let min_deposit_amount = read_u64(data, OFF_MIN_DEPOSIT_AMOUNT, "kvault.min_deposit_amount")?;
    let min_withdraw_amount = read_u64(data, OFF_MIN_WITHDRAW_AMOUNT, "kvault.min_withdraw_amount")?;
    let vault_withdrawal_penalty_lamports = read_u64(
        data,
        OFF_WITHDRAWAL_PENALTY_LAMPORTS,
        "kvault.withdrawal_penalty_lamports",
    )?;
    let vault_withdrawal_penalty_bps = read_u64(
        data,
        OFF_WITHDRAWAL_PENALTY_BPS,
        "kvault.withdrawal_penalty_bps",
    )?;

    let mut active_allocations = Vec::with_capacity(4);
    for i in 0..MAX_RESERVES {
        let base = OFF_VAULT_ALLOCATION_STRATEGY + i * VAULT_ALLOCATION_SIZE;
        let reserve = read_pubkey(data, base + ALLOC_OFF_RESERVE, "kvault.allocation.reserve")?;
        if reserve == Pubkey::default() {
            continue;
        }
        let ctoken_vault =
            read_pubkey(data, base + ALLOC_OFF_CTOKEN_VAULT, "kvault.allocation.ctoken_vault")?;
        let target_allocation_weight = read_u64(
            data,
            base + ALLOC_OFF_TARGET_ALLOCATION_WEIGHT,
            "kvault.allocation.target_weight",
        )?;
        let token_allocation_cap = read_u64(
            data,
            base + ALLOC_OFF_TOKEN_ALLOCATION_CAP,
            "kvault.allocation.token_cap",
        )?;
        let ctoken_allocation = read_u64(
            data,
            base + ALLOC_OFF_CTOKEN_ALLOCATION,
            "kvault.allocation.ctoken_allocation",
        )?;
        active_allocations.push(ActiveAllocation {
            reserve,
            ctoken_vault,
            target_allocation_weight,
            token_allocation_cap,
            ctoken_allocation,
            slot_index: i,
        });
    }

    Ok(KvaultState {
        token_mint,
        token_mint_decimals,
        token_vault,
        shares_mint,
        base_vault_authority,
        token_available,
        shares_issued,
        crank_fund_fee_per_reserve,
        performance_fee_bps,
        management_fee_bps,
        last_fee_charge_timestamp,
        prev_aum_sf,
        pending_fees_sf,
        min_deposit_amount,
        min_withdraw_amount,
        vault_withdrawal_penalty_lamports,
        vault_withdrawal_penalty_bps,
        active_allocations,
        global_withdrawal_penalty_lamports: 0,
        global_withdrawal_penalty_bps: 0,
        cached_reserves: Vec::new(),
        cached_farm_state: None,
        underlying_token_program: SPL_TOKEN_PROGRAM_ID,
        position_kvault_shares: 0,
        cached_aum_sf: None,
        cached_max_deposit: u64::MAX,
    })
}

pub fn decode_farm_state_minimal(data: &[u8]) -> Result<FarmStateMinimal, TradingVenueError> {
    if data.len() < FARM_MIN_LEN {
        return Err(TradingVenueError::DeserializationError(
            "kfarms FarmState: length".into(),
        ));
    }
    let farm_vault = read_pubkey(data, FARM_OFF_VAULT, "kfarms.farm_vault")?;
    let farm_vaults_authority =
        read_pubkey(data, FARM_OFF_VAULTS_AUTHORITY, "kfarms.farm_vaults_authority")?;
    let scope_raw = read_pubkey(data, FARM_OFF_SCOPE_PRICES, "kfarms.scope_prices")?;
    let scope_prices = if scope_raw == Pubkey::default() {
        None
    } else {
        Some(scope_raw)
    };
    let total_active_stake_scaled = read_u128(
        data,
        FARM_OFF_TOTAL_ACTIVE_STAKE_SCALED,
        "kfarms.total_active_stake_scaled",
    )?;
    let total_staked_amount = read_u64(
        data,
        FARM_OFF_TOTAL_STAKED_AMOUNT,
        "kfarms.total_staked_amount",
    )?;
    Ok(FarmStateMinimal {
        farm_vault,
        farm_vaults_authority,
        scope_prices,
        total_active_stake_scaled,
        total_staked_amount,
    })
}

pub fn convert_stake_to_amount(
    stake_scaled: u128,
    total_stake_scaled: u128,
    total_amount: u64,
) -> Result<u64, TradingVenueError> {
    const WAD: u128 = 1_000_000_000_000_000_000;
    if stake_scaled == 0 {
        return Ok(0);
    }
    if total_stake_scaled == 0 {
        return Ok(total_amount);
    }
    let result_scaled = U256::from(stake_scaled)
        .checked_mul(U256::from(WAD))
        .and_then(|x| x.checked_mul(U256::from(total_amount)))
        .and_then(|n| n.checked_div(U256::from(total_stake_scaled)))
        .ok_or(TradingVenueError::MathError("farm convert".into()))?;
    if result_scaled > (U256::one() << 192) - U256::one() {
        return Err(TradingVenueError::MathError("farm convert u192".into()));
    }
    let amount = result_scaled
        .checked_div(U256::from(WAD))
        .ok_or(TradingVenueError::MathError("farm convert wad".into()))?;
    if amount > U256::from(u64::MAX) {
        return Err(TradingVenueError::MathError("farm convert u64".into()));
    }
    Ok(amount.low_u64())
}

pub fn decode_global_config_penalty(data: &[u8]) -> Result<(u64, u64), TradingVenueError> {
    if data.len() < GLOBAL_CONFIG_LEN {
        return Err(TradingVenueError::DeserializationError(
            "kvault global_config: length".into(),
        ));
    }
    let lamports = read_u64(
        data,
        KGC_OFF_WITHDRAWAL_PENALTY_LAMPORTS,
        "kvault_gc.withdrawal_penalty_lamports",
    )?;
    let bps = read_u64(
        data,
        KGC_OFF_WITHDRAWAL_PENALTY_BPS,
        "kvault_gc.withdrawal_penalty_bps",
    )?;
    Ok((lamports, bps))
}

impl KvaultState {
    pub fn prev_aum(&self) -> Fraction {
        Fraction::from_bits(self.prev_aum_sf)
    }

    pub fn pending_fees(&self) -> Fraction {
        Fraction::from_bits(self.pending_fees_sf)
    }

    pub fn effective_penalty_lamports(&self) -> u64 {
        self.vault_withdrawal_penalty_lamports
            .max(self.global_withdrawal_penalty_lamports)
    }

    pub fn effective_penalty_bps(&self) -> u64 {
        self.vault_withdrawal_penalty_bps
            .max(self.global_withdrawal_penalty_bps)
    }
}
