use crate::your_venue::common::u256::U256;
use crate::trading_venue::error::TradingVenueError;

pub const FULL_BPS: u32 = 10_000;

pub fn require_no_value_per_share_loss(
    old_value: u128,
    old_supply: u64,
    new_value: u128,
    new_supply: u64,
) -> Result<(), TradingVenueError> {
    if old_supply == 0 || new_supply == 0 {
        return Ok(());
    }
    let lhs = U256::from(new_value) * U256::from(old_supply);
    let rhs = U256::from(old_value) * U256::from(new_supply);
    if lhs < rhs {
        return Err(TradingVenueError::MathError(
            "value per share decreased".into(),
        ));
    }
    Ok(())
}

pub fn mul_div_u64(a: u64, b: u64, denom: u64) -> Result<u64, TradingVenueError> {
    let prod = (a as u128)
        .checked_mul(b as u128)
        .ok_or(TradingVenueError::MathError("mul_div_u64 mul".into()))?;
    let quot = prod
        .checked_div(denom as u128)
        .ok_or(TradingVenueError::MathError("mul_div_u64 div".into()))?;
    u64::try_from(quot).map_err(|_| TradingVenueError::MathError("mul_div_u64 fit".into()))
}

pub fn u128_to_u64(v: u128) -> Result<u64, TradingVenueError> {
    u64::try_from(v).map_err(|_| TradingVenueError::MathError("u128_to_u64 fit".into()))
}

pub fn mul_bps(amount: u64, bps: u16) -> Result<u64, TradingVenueError> {
    mul_div_u64(amount, bps as u64, FULL_BPS as u64)
}
