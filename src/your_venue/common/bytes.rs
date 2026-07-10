use solana_pubkey::Pubkey;

use crate::trading_venue::error::TradingVenueError;

pub fn read_pubkey(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<Pubkey, TradingVenueError> {
    let end = offset
        .checked_add(32)
        .ok_or(TradingVenueError::DeserializationError(label.into()))?;
    if data.len() < end {
        return Err(TradingVenueError::DeserializationError(label.into()));
    }
    Pubkey::try_from(&data[offset..end])
        .map_err(|_| TradingVenueError::DeserializationError(label.into()))
}

pub fn read_u8(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<u8, TradingVenueError> {
    data.get(offset)
        .copied()
        .ok_or(TradingVenueError::DeserializationError(label.into()))
}

pub fn read_u16(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<u16, TradingVenueError> {
    let end = offset
        .checked_add(2)
        .ok_or(TradingVenueError::DeserializationError(label.into()))?;
    if data.len() < end {
        return Err(TradingVenueError::DeserializationError(label.into()));
    }
    Ok(u16::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| TradingVenueError::DeserializationError(label.into()))?,
    ))
}

pub fn read_u32(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<u32, TradingVenueError> {
    let end = offset
        .checked_add(4)
        .ok_or(TradingVenueError::DeserializationError(label.into()))?;
    if data.len() < end {
        return Err(TradingVenueError::DeserializationError(label.into()));
    }
    Ok(u32::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| TradingVenueError::DeserializationError(label.into()))?,
    ))
}

pub fn read_u64(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<u64, TradingVenueError> {
    let end = offset
        .checked_add(8)
        .ok_or(TradingVenueError::DeserializationError(label.into()))?;
    if data.len() < end {
        return Err(TradingVenueError::DeserializationError(label.into()));
    }
    Ok(u64::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| TradingVenueError::DeserializationError(label.into()))?,
    ))
}

pub fn read_u128(
    data: &[u8],
    offset: usize,
    label: &'static str,
) -> Result<u128, TradingVenueError> {
    let end = offset
        .checked_add(16)
        .ok_or(TradingVenueError::DeserializationError(label.into()))?;
    if data.len() < end {
        return Err(TradingVenueError::DeserializationError(label.into()));
    }
    Ok(u128::from_le_bytes(
        data[offset..end]
            .try_into()
            .map_err(|_| TradingVenueError::DeserializationError(label.into()))?,
    ))
}
