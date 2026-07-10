use solana_pubkey::Pubkey;

use super::wad::Wad;
use crate::your_venue::common::bytes::{read_pubkey as read_pk, read_u128, read_u64};
use crate::trading_venue::error::TradingVenueError;

pub const RESERVE_LEN: usize = 619;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct SaveState {
    pub last_update_slot: u64,
    pub lending_market: Pubkey,
    pub mint_pubkey: Pubkey,
    pub supply_vault: Pubkey,
    pub pyth_oracle: Pubkey,
    pub switchboard_oracle: Pubkey,
    pub available_amount: u64,
    pub borrowed_amount_wads: Wad,
    pub accumulated_protocol_fees_wads: Wad,
    pub collateral_mint_pubkey: Pubkey,
    pub collateral_supply_vault: Pubkey,
    pub collateral_mint_total_supply: u64,
    pub optimal_utilization_rate: u8,
    pub max_utilization_rate: u8,
    pub min_borrow_rate: u8,
    pub optimal_borrow_rate: u8,
    pub max_borrow_rate: u8,
    pub super_max_borrow_rate: u64,
    pub protocol_take_rate: u8,
    pub deposit_limit: u64,
    pub extra_oracle_pubkey: Pubkey,
}

pub fn decode(data: &[u8]) -> Result<SaveState, TradingVenueError> {
    if data.len() < RESERVE_LEN {
        return Err(TradingVenueError::DeserializationError(
            "save reserve: length".into(),
        ));
    }

    Ok(SaveState {
        last_update_slot: read_u64(data, 1, "save.last_update.slot")?,
        lending_market: read_pk(data, 10, "save.lending_market")?,
        mint_pubkey: read_pk(data, 42, "save.liquidity.mint_pubkey")?,
        supply_vault: read_pk(data, 75, "save.liquidity.supply_pubkey")?,
        pyth_oracle: read_pk(data, 107, "save.liquidity.pyth_oracle")?,
        switchboard_oracle: read_pk(data, 139, "save.liquidity.switchboard_oracle")?,
        available_amount: read_u64(data, 171, "save.liquidity.available_amount")?,
        borrowed_amount_wads: read_u128(data, 179, "save.liquidity.borrowed_amount_wads")?,
        accumulated_protocol_fees_wads: read_u128(
            data,
            373,
            "save.liquidity.accumulated_protocol_fees_wads",
        )?,
        collateral_mint_pubkey: read_pk(data, 227, "save.collateral.mint_pubkey")?,
        collateral_supply_vault: read_pk(data, 267, "save.collateral.supply_pubkey")?,
        collateral_mint_total_supply: read_u64(data, 259, "save.collateral.mint_total_supply")?,
        optimal_utilization_rate: data[299],
        min_borrow_rate: data[303],
        optimal_borrow_rate: data[304],
        max_borrow_rate: data[305],
        deposit_limit: read_u64(data, 323, "save.config.deposit_limit")?,
        protocol_take_rate: data[372],
        max_utilization_rate: data[470],
        super_max_borrow_rate: read_u64(data, 471, "save.config.super_max_borrow_rate")?,
        extra_oracle_pubkey: read_pk(data, 489, "save.config.extra_oracle_pubkey")?,
    })
}
