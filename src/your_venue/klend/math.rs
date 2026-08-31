use super::fraction::{
    BigFraction, Fraction, approximate_compounded_interest, bps_to_fraction, percent_to_fraction,
};
use super::state::{BORROW_RATE_CURVE_LEN, KlendState};
use super::withdrawal_caps;
use crate::your_venue::common::checked::{
    FULL_BPS, mul_bps, require_no_value_per_share_loss, u128_to_u64,
};
use crate::your_venue::common::u256::U256;
use crate::your_venue::state::{GlobalConfig, WrapperVault};
use crate::trading_venue::error::TradingVenueError;

pub const POINTS_IN_CURVE: usize = 11;
pub const POINT_SIZE: usize = 8;
pub const SLOTS_PER_YEAR: u64 = 63_072_000;
pub const SECONDS_PER_YEAR: u64 = 31_536_000;
pub const PROGRAM_VERSION: u64 = 1;

const INTEREST_RATE_BASIS_LEGACY: u8 = 0;
const INTEREST_RATE_BASIS_TRUE_APR: u8 = 1;

#[derive(Debug, Clone, Copy)]
pub struct AccruedKlend {
    pub borrowed_amount_sf: u128,
    pub accumulated_protocol_fees_sf: u128,
    pub pending_referrer_fees_sf: u128,
    pub accumulated_referrer_fees_sf: u128,
}

fn lookup_borrow_rate(curve: &[u8; BORROW_RATE_CURVE_LEN], utilization: Fraction) -> Fraction {
    let utilization = if utilization > Fraction::ONE {
        Fraction::ONE
    } else {
        utilization
    };
    let util_bps_u32: u32 = (utilization * Fraction::from_num(FULL_BPS))
        .round()
        .to_num();
    let util_bps = util_bps_u32.min(FULL_BPS);

    let point = |i: usize| -> (u32, u32) {
        let off = i * POINT_SIZE;
        let util = u32::from_le_bytes([curve[off], curve[off + 1], curve[off + 2], curve[off + 3]]);
        let rate = u32::from_le_bytes([
            curve[off + 4],
            curve[off + 5],
            curve[off + 6],
            curve[off + 7],
        ]);
        (util, rate)
    };

    let (mut prev_util, mut prev_rate) = point(0);
    if util_bps == prev_util {
        return bps_to_fraction(prev_rate);
    }
    for i in 1..POINTS_IN_CURVE {
        let (cur_util, cur_rate) = point(i);
        if util_bps == cur_util {
            return bps_to_fraction(cur_rate);
        }
        if util_bps > prev_util && util_bps < cur_util {
            let slope_nom = cur_rate.saturating_sub(prev_rate);
            let slope_denom = cur_util - prev_util;
            let start_util_f = bps_to_fraction(prev_util);
            let coef = utilization - start_util_f;
            let nom = coef * u128::from(slope_nom);
            let base_rate = nom / u128::from(slope_denom);
            let offset = bps_to_fraction(prev_rate);
            return base_rate + offset;
        }
        prev_util = cur_util;
        prev_rate = cur_rate;
    }
    bps_to_fraction(prev_rate)
}

fn total_supply_core(
    total_available_amount: u64,
    borrowed_amount_sf: u128,
    accumulated_protocol_fees_sf: u128,
    accumulated_referrer_fees_sf: u128,
    pending_referrer_fees_sf: u128,
) -> Fraction {
    Fraction::from_num(total_available_amount) + Fraction::from_bits(borrowed_amount_sf)
        - Fraction::from_bits(accumulated_protocol_fees_sf)
        - Fraction::from_bits(accumulated_referrer_fees_sf)
        - Fraction::from_bits(pending_referrer_fees_sf)
}

fn total_supply_fraction_pre(state: &KlendState) -> Fraction {
    total_supply_core(
        state.total_available_amount,
        state.borrowed_amount_sf,
        state.accumulated_protocol_fees_sf,
        state.accumulated_referrer_fees_sf,
        state.pending_referrer_fees_sf,
    )
}

pub fn total_supply_fraction(state: &KlendState, accrued: &AccruedKlend) -> Fraction {
    total_supply_core(
        state.total_available_amount,
        accrued.borrowed_amount_sf,
        accrued.accumulated_protocol_fees_sf,
        accrued.accumulated_referrer_fees_sf,
        accrued.pending_referrer_fees_sf,
    )
}

fn utilization_rate(borrowed_sf: u128, total_supply: Fraction) -> Fraction {
    if total_supply == Fraction::ZERO {
        return Fraction::ZERO;
    }
    Fraction::from_bits(borrowed_sf) / total_supply
}

struct AccrualDuration {
    elapsed_units: u64,
    units_per_year: u128,
}

impl AccrualDuration {
    fn since(
        basis: u8,
        last_update_slot: u64,
        last_update_timestamp: u64,
        current_slot: u64,
        unix_timestamp: i64,
    ) -> Result<Self, TradingVenueError> {
        match basis {
            INTEREST_RATE_BASIS_LEGACY => Ok(Self {
                elapsed_units: current_slot.saturating_sub(last_update_slot),
                units_per_year: SLOTS_PER_YEAR as u128,
            }),
            INTEREST_RATE_BASIS_TRUE_APR => {
                let now = unix_timestamp.max(0) as u64;
                let elapsed_units = if last_update_timestamp == 0 {
                    0
                } else {
                    now.saturating_sub(last_update_timestamp)
                };
                Ok(Self {
                    elapsed_units,
                    units_per_year: SECONDS_PER_YEAR as u128,
                })
            }
            _ => Err(TradingVenueError::MathError(
                "unknown interest rate basis".into(),
            )),
        }
    }

    fn is_zero(&self) -> bool {
        self.elapsed_units == 0
    }
}

pub fn accrue_interest(
    state: &KlendState,
    current_slot: u64,
    unix_timestamp: i64,
) -> Result<AccruedKlend, TradingVenueError> {
    let duration = AccrualDuration::since(
        state.interest_rate_basis,
        state.last_update_slot,
        state.last_update_timestamp,
        current_slot,
        unix_timestamp,
    )?;
    if duration.is_zero() {
        return Ok(AccruedKlend {
            borrowed_amount_sf: state.borrowed_amount_sf,
            accumulated_protocol_fees_sf: state.accumulated_protocol_fees_sf,
            pending_referrer_fees_sf: state.pending_referrer_fees_sf,
            accumulated_referrer_fees_sf: state.accumulated_referrer_fees_sf,
        });
    }

    let pre_total_supply = total_supply_fraction_pre(state);
    let utilization = utilization_rate(state.borrowed_amount_sf, pre_total_supply);
    let current_borrow_rate = lookup_borrow_rate(&state.borrow_rate_curve, utilization);
    let host_fixed_interest_rate = bps_to_fraction(state.host_fixed_interest_rate_bps.into());
    let protocol_take_rate = percent_to_fraction(state.protocol_take_rate_pct);
    let referral_rate = Fraction::ZERO;

    let compounded_interest_rate = approximate_compounded_interest(
        current_borrow_rate + host_fixed_interest_rate,
        duration.elapsed_units,
        duration.units_per_year,
    );
    let compounded_fixed_rate = approximate_compounded_interest(
        host_fixed_interest_rate,
        duration.elapsed_units,
        duration.units_per_year,
    );

    let previous_debt_f = Fraction::from_bits(state.borrowed_amount_sf);
    let acc_protocol_fees_f = Fraction::from_bits(state.accumulated_protocol_fees_sf);

    let new_debt_f = previous_debt_f * compounded_interest_rate;
    let fixed_host_fee = (previous_debt_f * compounded_fixed_rate) - previous_debt_f;
    let net_new_variable_debt_f = new_debt_f - previous_debt_f - fixed_host_fee;
    let variable_protocol_fee_f = net_new_variable_debt_f * protocol_take_rate;
    let absolute_referral_rate = protocol_take_rate * referral_rate;
    let max_referrers_fees_f = net_new_variable_debt_f * absolute_referral_rate;

    let new_acc_protocol_fees_f =
        acc_protocol_fees_f + fixed_host_fee + variable_protocol_fee_f - max_referrers_fees_f;

    Ok(AccruedKlend {
        borrowed_amount_sf: new_debt_f.to_bits(),
        accumulated_protocol_fees_sf: new_acc_protocol_fees_f.to_bits(),
        pending_referrer_fees_sf: state.pending_referrer_fees_sf + max_referrers_fees_f.to_bits(),
        accumulated_referrer_fees_sf: state.accumulated_referrer_fees_sf,
    })
}

pub fn pre_accrue(
    state: &mut KlendState,
    current_slot: u64,
    unix_timestamp: i64,
) -> Result<(), TradingVenueError> {
    let accrued = accrue_interest(state, current_slot, unix_timestamp)?;
    state.borrowed_amount_sf = accrued.borrowed_amount_sf;
    state.accumulated_protocol_fees_sf = accrued.accumulated_protocol_fees_sf;
    state.pending_referrer_fees_sf = accrued.pending_referrer_fees_sf;
    state.accumulated_referrer_fees_sf = accrued.accumulated_referrer_fees_sf;
    state.last_update_slot = current_slot;
    state.last_update_timestamp = unix_timestamp.max(0) as u64;
    Ok(())
}

pub fn exchange_rate_pair(collateral_supply: u64, total_supply: Fraction) -> (u128, Fraction) {
    if collateral_supply == 0 || total_supply == Fraction::ZERO {
        (1, Fraction::ONE)
    } else {
        (collateral_supply as u128, total_supply)
    }
}

fn liquidity_to_collateral(
    liquidity_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, TradingVenueError> {
    let lhs = collateral_supply
        .checked_mul(liquidity_amount as u128)
        .ok_or(TradingVenueError::MathError("liq_to_col mul".into()))?;
    let big = BigFraction::from_num(lhs)
        .ok_or(TradingVenueError::MathError("liq_to_col from_num".into()))?;
    let rhs = BigFraction::from(total_supply);
    let result = big
        .checked_div(rhs)
        .ok_or(TradingVenueError::MathError("liq_to_col div".into()))?;
    let fraction = result
        .try_into_fraction()
        .ok_or(TradingVenueError::MathError("liq_to_col convert".into()))?;
    let floored: u128 = fraction.floor().to_num();
    u128_to_u64(floored)
}

fn collateral_to_liquidity_ceil(
    collateral_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, TradingVenueError> {
    if collateral_supply == 0 {
        return Err(TradingVenueError::MathError("cs=0".into()));
    }
    let coll = U256::from(collateral_amount);
    let liquidity_sbf = U256::from(total_supply.to_bits());
    let cs = U256::from(collateral_supply);

    let product = coll
        .checked_mul(liquidity_sbf)
        .ok_or(TradingVenueError::MathError("ceil mul".into()))?;
    let numerator = product
        .checked_add(cs - U256::one())
        .ok_or(TradingVenueError::MathError("ceil add".into()))?;
    let quotient = numerator / cs;

    let bits: u128 = quotient
        .try_into()
        .map_err(|_| TradingVenueError::MathError("ceil convert".into()))?;
    let fraction = Fraction::from_bits(bits);
    let ceiled: u128 = fraction.ceil().to_num();
    u128_to_u64(ceiled)
}

fn collateral_to_liquidity(
    collateral_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, TradingVenueError> {
    if collateral_supply == 0 {
        return Err(TradingVenueError::MathError("cs=0".into()));
    }
    let coll = U256::from(collateral_amount);
    let liquidity_sbf = U256::from(total_supply.to_bits());
    let cs = U256::from(collateral_supply);

    let product = coll
        .checked_mul(liquidity_sbf)
        .ok_or(TradingVenueError::MathError("col_to_liq mul".into()))?;
    let quotient = product / cs;

    let bits: u128 = quotient
        .try_into()
        .map_err(|_| TradingVenueError::MathError("col_to_liq convert".into()))?;
    let fraction = Fraction::from_bits(bits);
    let floored: u128 = fraction.floor().to_num();
    u128_to_u64(floored)
}

pub fn fraction_collateral_to_liquidity(
    collateral_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<Fraction, TradingVenueError> {
    if collateral_supply == 0 {
        return Err(TradingVenueError::MathError("cs=0".into()));
    }
    let coll = U256::from(collateral_amount);
    let liquidity_sbf = U256::from(total_supply.to_bits());
    let cs = U256::from(collateral_supply);
    let product = coll
        .checked_mul(liquidity_sbf)
        .ok_or(TradingVenueError::MathError("frac mul".into()))?;
    let quotient = product / cs;
    let bits: u128 = quotient
        .try_into()
        .map_err(|_| TradingVenueError::MathError("frac convert".into()))?;
    Ok(Fraction::from_bits(bits))
}

pub fn fraction_liquidity_to_collateral_ceil(
    liquidity_amount: u64,
    collateral_supply: u128,
    total_supply: Fraction,
) -> Result<u64, TradingVenueError> {
    if collateral_supply == 0 {
        return Err(TradingVenueError::MathError("cs=0".into()));
    }
    let den = U256::from(total_supply.to_bits());
    if den.is_zero() {
        return Err(TradingVenueError::MathError("ts=0".into()));
    }
    let num = U256::from(liquidity_amount)
        .checked_mul(U256::from(collateral_supply))
        .ok_or(TradingVenueError::MathError("l2c mul".into()))?
        .checked_mul(U256::from(1u128 << 60))
        .ok_or(TradingVenueError::MathError("l2c shift".into()))?;
    let quotient = (num + den - U256::one()) / den;
    let v: u128 = quotient
        .try_into()
        .map_err(|_| TradingVenueError::MathError("l2c convert".into()))?;
    u128_to_u64(v)
}

pub fn quote_deposit(
    state: &KlendState,
    current_slot: u64,
    unix_timestamp: i64,
    wv: &WrapperVault,
    gc: &GlobalConfig,
    amount: u64,
) -> Result<u64, TradingVenueError> {
    if amount == 0 {
        return Err(TradingVenueError::MathError("zero amount".into()));
    }
    if state.emergency_mode {
        return Err(TradingVenueError::MathError("emergency mode".into()));
    }
    if state.version != PROGRAM_VERSION {
        return Err(TradingVenueError::MathError("reserve deprecated".into()));
    }
    if state.status != 0 {
        return Err(TradingVenueError::MathError("reserve obsolete".into()));
    }
    if state.block_ctoken_usage {
        return Err(TradingVenueError::MathError("ctoken usage blocked".into()));
    }

    let accrued = accrue_interest(state, current_slot, unix_timestamp)?;
    let total_supply_f = total_supply_fraction(state, &accrued);
    let collateral_supply = state.collateral_mint_total_supply;

    let protocol_fee = mul_bps(amount, gc.protocol_deposit_fee_bps)?;
    let creator_fee = mul_bps(amount, wv.creator_deposit_fee_bps)?;
    let net_amount = amount
        .checked_sub(protocol_fee)
        .and_then(|x| x.checked_sub(creator_fee))
        .ok_or(TradingVenueError::MathError("net amount".into()))?;
    if net_amount == 0 {
        return Err(TradingVenueError::MathError("net zero".into()));
    }

    let (cs, ts) = exchange_rate_pair(collateral_supply, total_supply_f);

    let collateral_amount = liquidity_to_collateral(net_amount, cs, ts)?;
    if collateral_amount == 0 {
        return Err(TradingVenueError::MathError("collateral zero".into()));
    }
    let liquidity_amount_to_deposit = collateral_to_liquidity_ceil(collateral_amount, cs, ts)?;
    if liquidity_amount_to_deposit > net_amount {
        return Err(TradingVenueError::MathError("deposit > net".into()));
    }

    let new_supply_f = Fraction::from_num(liquidity_amount_to_deposit) + total_supply_f;
    if new_supply_f > Fraction::from_num(state.deposit_limit) {
        return Err(TradingVenueError::MathError("deposit limit".into()));
    }

    let wrapper_supply = wv.wrapper_supply;
    let prev_intermediate = wv.intermediate_held;
    let wrapper_to_mint = if wrapper_supply == 0 {
        net_amount
    } else {
        if prev_intermediate == 0 {
            return Err(TradingVenueError::MathError("klend ctokens out of sync".into()));
        }
        u128_to_u64(
            (collateral_amount as u128)
                .checked_mul(wrapper_supply as u128)
                .ok_or(TradingVenueError::MathError("wrap mul".into()))?
                .checked_div(prev_intermediate)
                .ok_or(TradingVenueError::MathError("wrap div".into()))?,
        )?
    };
    if wrapper_to_mint == 0 {
        return Err(TradingVenueError::MathError("mint zero".into()));
    }

    let new_intermediate = prev_intermediate
        .checked_add(collateral_amount as u128)
        .ok_or(TradingVenueError::MathError("new ctokens total".into()))?;
    let new_wrapper_supply = wrapper_supply
        .checked_add(wrapper_to_mint)
        .ok_or(TradingVenueError::MathError("new supply".into()))?;
    require_no_value_per_share_loss(
        prev_intermediate,
        wrapper_supply,
        new_intermediate,
        new_wrapper_supply,
    )?;

    Ok(wrapper_to_mint)
}

pub fn quote_withdraw(
    state: &KlendState,
    current_slot: u64,
    unix_timestamp: i64,
    wv: &WrapperVault,
    amount: u64,
) -> Result<u64, TradingVenueError> {
    if amount == 0 {
        return Err(TradingVenueError::MathError("zero amount".into()));
    }
    if state.emergency_mode {
        return Err(TradingVenueError::MathError("emergency mode".into()));
    }
    if state.version != PROGRAM_VERSION {
        return Err(TradingVenueError::MathError("reserve deprecated".into()));
    }
    if amount > wv.wrapper_supply {
        return Err(TradingVenueError::MathError("burn exceeds supply".into()));
    }

    let total_ctokens = wv.intermediate_held;
    if total_ctokens == 0 {
        return Err(TradingVenueError::MathError("no ctokens".into()));
    }
    let ctokens_to_redeem = u128_to_u64(
        (amount as u128)
            .checked_mul(total_ctokens)
            .ok_or(TradingVenueError::MathError("redeem mul".into()))?
            .checked_div(wv.wrapper_supply as u128)
            .ok_or(TradingVenueError::MathError("redeem div".into()))?,
    )?;
    if ctokens_to_redeem == 0 {
        return Err(TradingVenueError::MathError("ctoken zero".into()));
    }

    let accrued = accrue_interest(state, current_slot, unix_timestamp)?;
    let total_supply_f = total_supply_fraction(state, &accrued);
    let (cs, ts) = exchange_rate_pair(state.collateral_mint_total_supply, total_supply_f);

    let underlying_received = collateral_to_liquidity(ctokens_to_redeem, cs, ts)?;
    if underlying_received == 0 {
        return Err(TradingVenueError::MathError("underlying zero".into()));
    }

    let queued_liquidity = if state.queued_collateral_amount == 0 {
        0
    } else {
        collateral_to_liquidity(state.queued_collateral_amount, cs, ts)?
    };
    let freely_available = state
        .total_available_amount
        .saturating_sub(queued_liquidity);
    if underlying_received > freely_available {
        return Err(TradingVenueError::MathError("insufficient liquidity".into()));
    }

    let remaining = withdrawal_caps::remaining_amount(
        &state.deposit_withdrawal_cap,
        unix_timestamp.max(0) as u64,
    );
    if underlying_received > remaining {
        return Err(TradingVenueError::MathError("withdrawal cap".into()));
    }

    let new_intermediate = total_ctokens
        .checked_sub(ctokens_to_redeem as u128)
        .ok_or(TradingVenueError::MathError("new ctokens total".into()))?;
    let new_wrapper_supply = wv
        .wrapper_supply
        .checked_sub(amount)
        .ok_or(TradingVenueError::MathError("new supply".into()))?;
    require_no_value_per_share_loss(
        total_ctokens,
        wv.wrapper_supply,
        new_intermediate,
        new_wrapper_supply,
    )?;

    Ok(underlying_received)
}
