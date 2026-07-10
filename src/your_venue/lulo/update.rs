use solana_pubkey::Pubkey;

use super::account_metas;
use super::state::{self, LuloState};
use crate::account_caching::AccountsCache;
use crate::your_venue::state::WrapperVault;
use crate::trading_venue::error::TradingVenueError;

pub fn required_pubkeys(wv: &WrapperVault) -> Vec<Pubkey> {
    vec![
        wv.source_pool,
        wv.source_position_pda,
        account_metas::referrer_pool_user(),
    ]
}

pub async fn run(
    s: &mut LuloState,
    wv: &WrapperVault,
    cache: &dyn AccountsCache,
) -> Result<(), TradingVenueError> {
    let pool = cache
        .get_account(&wv.source_pool)
        .await?
        .ok_or(TradingVenueError::NoAccountFound(wv.source_pool.into()))?;
    *s = state::decode(&pool.data)?;

    let pool_user = cache
        .get_account(&wv.source_position_pda)
        .await?
        .ok_or(TradingVenueError::NoAccountFound(
            wv.source_position_pda.into(),
        ))?;
    s.basis_q60 = state::decode_basis_q60(&pool_user.data)?;
    let (charged, avg) = state::decode_charged_avg_q60(&pool_user.data)?;
    s.charged_q60 = charged;
    s.avg_q60 = avg;

    let referrer = account_metas::referrer_pool_user();
    let referrer_acct = cache
        .get_account(&referrer)
        .await?
        .ok_or(TradingVenueError::NoAccountFound(referrer.into()))?;
    s.ref_bps = state::decode_ref_bps(&referrer_acct.data)?;
    Ok(())
}
