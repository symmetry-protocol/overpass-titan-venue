use crate::your_venue::common::u256::U256;
use crate::trading_venue::error::TradingVenueError;

pub const WAD: u128 = 1_000_000_000_000_000_000;
pub const SLOTS_PER_YEAR: u64 = 63_072_000;

pub type Wad = u128;

#[inline]
pub fn from_u64(amount: u64) -> Wad {
    (amount as u128).saturating_mul(WAD)
}

#[inline]
pub fn from_percent(pct: u8) -> Wad {
    (pct as u128).saturating_mul(WAD / 100)
}

#[inline]
pub fn to_floor_u64(wad: Wad) -> u64 {
    (wad / WAD) as u64
}

#[inline]
#[allow(dead_code)]
pub fn to_ceil_u64(wad: Wad) -> u64 {
    (wad.saturating_add(WAD - 1) / WAD) as u64
}

pub fn wad_mul(a: Wad, b: Wad) -> Result<Wad, TradingVenueError> {
    let product = U256::from(a) * U256::from(b);
    let scaled = product / U256::from(WAD);
    scaled
        .try_into()
        .map_err(|_| TradingVenueError::MathError("wad_mul overflow".into()))
}

pub fn wad_div(a: Wad, b: Wad) -> Result<Wad, TradingVenueError> {
    if b == 0 {
        return Err(TradingVenueError::MathError("wad_div by zero".into()));
    }
    let scaled = U256::from(a) * U256::from(WAD);
    let quot = scaled / U256::from(b);
    quot.try_into()
        .map_err(|_| TradingVenueError::MathError("wad_div overflow".into()))
}

pub fn wad_pow(base: Wad, mut exponent: u64) -> Result<Wad, TradingVenueError> {
    let mut base = base;
    let mut ret: Wad = if exponent & 1 != 0 { base } else { WAD };
    loop {
        exponent >>= 1;
        if exponent == 0 {
            break;
        }
        base = wad_mul(base, base)?;
        if exponent & 1 != 0 {
            ret = wad_mul(ret, base)?;
        }
    }
    Ok(ret)
}
