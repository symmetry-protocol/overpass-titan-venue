use solana_pubkey::Pubkey;

use crate::your_venue::state::WrapperVault;
use crate::trading_venue::error::TradingVenueError;

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Deposit,
    Withdraw,
}

pub fn detect(
    wv: &WrapperVault,
    input: Pubkey,
    output: Pubkey,
) -> Result<Direction, TradingVenueError> {
    if input == wv.underlying_mint && output == wv.wrapper_mint {
        Ok(Direction::Deposit)
    } else if input == wv.wrapper_mint && output == wv.underlying_mint {
        Ok(Direction::Withdraw)
    } else {
        Err(TradingVenueError::InvalidMint("unsupported mint pair".into()))
    }
}
