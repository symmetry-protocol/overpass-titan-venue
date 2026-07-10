mod account_metas;
mod math;
mod state;
mod update;
mod wad;

pub use state::SaveState;

use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

use crate::{
    account_caching::AccountsCache,
    your_venue::state::{GlobalConfig, WrapperVault},
    trading_venue::error::TradingVenueError,
};

pub const SAVE_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo");

pub const DEPOSIT_IX_DISCRIMINATOR: [u8; 8] = [0xe4, 0x76, 0x53, 0x1d, 0xb1, 0xf3, 0x08, 0x78];
pub const WITHDRAW_IX_DISCRIMINATOR: [u8; 8] = [0x3c, 0x22, 0x09, 0x4d, 0x0c, 0x77, 0x42, 0x69];

#[derive(Clone, Default)]
pub struct Save {
    state: SaveState,
    current_slot: u64,
}

impl Save {
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
        update::run(&mut self.state, &mut self.current_slot, wv, cache).await
    }

    pub fn quote_deposit(
        &self,
        wv: &WrapperVault,
        gc: &GlobalConfig,
        amount: u64,
    ) -> Result<u64, TradingVenueError> {
        math::quote_deposit(&self.state, self.current_slot, wv, gc, amount)
    }

    pub fn quote_withdraw(
        &self,
        wv: &WrapperVault,
        amount: u64,
    ) -> Result<u64, TradingVenueError> {
        math::quote_withdraw(&self.state, self.current_slot, wv, amount)
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
        let mut keys = vec![
            SAVE_PROGRAM_ID,
            wv.source_pool,
            self.state.lending_market,
            self.state.supply_vault,
            self.state.pyth_oracle,
            self.state.switchboard_oracle,
        ];
        if self.state.extra_oracle_pubkey != Pubkey::default() {
            keys.push(self.state.extra_oracle_pubkey);
        }
        keys
    }
}
