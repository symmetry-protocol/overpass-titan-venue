mod account_metas;
mod math;
mod state;
mod update;

pub use state::LuloState;

use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

use crate::{
    account_caching::AccountsCache,
    your_venue::state::{GlobalConfig, WrapperVault},
    trading_venue::error::TradingVenueError,
};

pub const LULO_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("FL3X2pRsQ9zHENpZSKDRREtccwJuei8yg9fwDu9UN69Q");

pub const DEPOSIT_IX_DISCRIMINATOR: [u8; 8] = [0xe7, 0x17, 0x29, 0x7c, 0xe2, 0x0c, 0x4a, 0xef];
pub const WITHDRAW_IX_DISCRIMINATOR: [u8; 8] = [0xf4, 0xa6, 0xeb, 0x1b, 0x92, 0xf2, 0x79, 0x23];

#[derive(Clone, Default)]
pub struct Lulo {
    state: LuloState,
}

impl Lulo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn required_pubkeys(&self, wv: &WrapperVault) -> Vec<Pubkey> {
        update::required_pubkeys(wv)
    }

    pub async fn update(
        &mut self,
        wv: &WrapperVault,
        cache: &dyn AccountsCache,
    ) -> Result<(), TradingVenueError> {
        update::run(&mut self.state, wv, cache).await
    }

    pub fn quote_deposit(
        &self,
        wv: &WrapperVault,
        gc: &GlobalConfig,
        amount: u64,
    ) -> Result<u64, TradingVenueError> {
        math::quote_deposit(&self.state, wv, gc, amount)
    }

    pub fn quote_withdraw(
        &self,
        wv: &WrapperVault,
        amount: u64,
    ) -> Result<u64, TradingVenueError> {
        math::quote_withdraw(&self.state, wv, amount)
    }

    pub fn build_deposit_ix(
        &self,
        wv: &WrapperVault,
        user: Pubkey,
        in_amount: u64,
        min_out: u64,
    ) -> Result<Instruction, TradingVenueError> {
        let accounts = account_metas::build_deposit(&self.state, wv, &user)?;
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&DEPOSIT_IX_DISCRIMINATOR);
        data.extend_from_slice(&in_amount.to_le_bytes());
        data.extend_from_slice(&min_out.to_le_bytes());
        Ok(Instruction {
            program_id: crate::your_venue::OVERPASS_PROGRAM_ID,
            accounts,
            data,
        })
    }

    pub fn build_withdraw_ix(
        &self,
        wv: &WrapperVault,
        user: Pubkey,
        in_amount: u64,
        min_out: u64,
    ) -> Result<Instruction, TradingVenueError> {
        let accounts = account_metas::build_withdraw(&self.state, wv, &user)?;
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&WITHDRAW_IX_DISCRIMINATOR);
        data.extend_from_slice(&in_amount.to_le_bytes());
        data.extend_from_slice(&min_out.to_le_bytes());
        Ok(Instruction {
            program_id: crate::your_venue::OVERPASS_PROGRAM_ID,
            accounts,
            data,
        })
    }

    pub fn lookup_table_keys(&self, wv: &WrapperVault) -> Vec<Pubkey> {
        vec![
            LULO_PROGRAM_ID,
            account_metas::JUP_LIQUIDITY_PROGRAM,
            account_metas::JUP_PROGRAM,
            wv.source_pool,
            wv.source_position_pda,
        ]
    }
}
