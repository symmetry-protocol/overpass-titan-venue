mod common;
mod klend;
mod kvault;
mod lulo;
mod marginfi;
mod save;
mod state;

use async_trait::async_trait;
use solana_account::Account;
use solana_instruction::Instruction;
use solana_pubkey::Pubkey;

use crate::{
    account_caching::AccountsCache,
    trading_venue::{
        AddressLookupTableTrait, FromAccount, QuoteRequest, QuoteResult, SwapType, TradingVenue,
        error::TradingVenueError, protocol::PoolProtocol, token_info::TokenInfo,
        venue_creation::{ParsedInstruction, PoolCreation},
    },
};

use common::direction::{self, Direction};

pub use klend::Klend;
pub use kvault::Kvault;
pub use lulo::Lulo;
pub use marginfi::Marginfi;
pub use save::Save;
pub use state::{GlobalConfig, ProtocolId, WrapperVault};

pub const OVERPASS_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("WRAPdXmxrH37RKUbH1QMnYrKdNe8w4Kz44t1cXmYeum");

pub fn parse_pool_creations(instructions: &[ParsedInstruction]) -> Vec<PoolCreation> {
    const WRAPPER_MINT_INDEX: usize = 3;
    const WRAPPER_VAULT_INDEX: usize = 4;
    const CREATE_KLEND: [u8; 8] = [136, 245, 198, 59, 16, 197, 150, 217];
    const CREATE_KVAULT: [u8; 8] = [43, 164, 2, 34, 109, 244, 81, 133];
    const CREATE_SAVE: [u8; 8] = [187, 66, 56, 163, 144, 111, 223, 237];
    const CREATE_MARGINFI: [u8; 8] = [240, 19, 160, 156, 172, 225, 176, 81];
    const CREATE_LULO: [u8; 8] = [170, 183, 146, 74, 209, 199, 235, 168];
    const CREATIONS: [([u8; 8], usize); 5] = [
        (CREATE_KLEND, 7),
        (CREATE_KVAULT, 7),
        (CREATE_SAVE, 7),
        (CREATE_MARGINFI, 9),
        (CREATE_LULO, 9),
    ];

    instructions
        .iter()
        .filter(|ix| ix.program_id == OVERPASS_PROGRAM_ID)
        .filter_map(|ix| {
            let disc: [u8; 8] = ix.data.get(..8)?.try_into().ok()?;
            let underlying_index = CREATIONS.iter().find(|(d, _)| *d == disc).map(|&(_, i)| i)?;
            let pool = *ix.accounts.get(WRAPPER_VAULT_INDEX)?;
            let wrapper_mint = *ix.accounts.get(WRAPPER_MINT_INDEX)?;
            let underlying_mint = *ix.accounts.get(underlying_index)?;
            Some(PoolCreation {
                protocol: PoolProtocol::Overpass,
                pool,
                mints: vec![underlying_mint, wrapper_mint],
            })
        })
        .collect()
}

#[derive(Clone)]
pub struct OverpassVenue {
    wrapper_vault_pda: Pubkey,
    wrapper_vault: Option<WrapperVault>,
    global_config: Option<GlobalConfig>,
    token_info: [TokenInfo; 2],
    source: Option<Source>,
    parts_deposit: Vec<(u64, f64)>,
    parts_withdraw: Vec<(u64, f64)>,
}

#[derive(Clone)]
pub enum Source {
    Klend(Klend),
    Kvault(Kvault),
    Save(Save),
    Marginfi(Marginfi),
    Lulo(Lulo),
}

impl Source {
    pub fn required_pubkeys(&self, wv: &WrapperVault) -> Vec<Pubkey> {
        match self {
            Source::Klend(s) => s.required_pubkeys(wv),
            Source::Kvault(s) => s.required_pubkeys(wv),
            Source::Save(s) => s.required_pubkeys(wv),
            Source::Marginfi(s) => s.required_pubkeys(wv),
            Source::Lulo(s) => s.required_pubkeys(wv),
        }
    }

    pub async fn update(
        &mut self,
        wv: &WrapperVault,
        cache: &dyn AccountsCache,
    ) -> Result<(), TradingVenueError> {
        match self {
            Source::Klend(s) => s.update(wv, cache).await,
            Source::Kvault(s) => s.update(wv, cache).await,
            Source::Save(s) => s.update(wv, cache).await,
            Source::Marginfi(s) => s.update(wv, cache).await,
            Source::Lulo(s) => s.update(wv, cache).await,
        }
    }

    pub fn quote_deposit(
        &self,
        wv: &WrapperVault,
        gc: &GlobalConfig,
        amount: u64,
    ) -> Result<u64, TradingVenueError> {
        match self {
            Source::Klend(s) => s.quote_deposit(wv, gc, amount),
            Source::Kvault(s) => s.quote_deposit(wv, gc, amount),
            Source::Save(s) => s.quote_deposit(wv, gc, amount),
            Source::Marginfi(s) => s.quote_deposit(wv, gc, amount),
            Source::Lulo(s) => s.quote_deposit(wv, gc, amount),
        }
    }

    pub fn quote_withdraw(
        &self,
        wv: &WrapperVault,
        amount: u64,
    ) -> Result<u64, TradingVenueError> {
        match self {
            Source::Klend(s) => s.quote_withdraw(wv, amount),
            Source::Kvault(s) => s.quote_withdraw(wv, amount),
            Source::Save(s) => s.quote_withdraw(wv, amount),
            Source::Marginfi(s) => s.quote_withdraw(wv, amount),
            Source::Lulo(s) => s.quote_withdraw(wv, amount),
        }
    }

    pub fn build_deposit_ix(
        &self,
        wv: &WrapperVault,
        user: Pubkey,
        in_amount: u64,
        min_out: u64,
    ) -> Result<Instruction, TradingVenueError> {
        match self {
            Source::Klend(s) => s.build_deposit_ix(wv, user, in_amount, min_out),
            Source::Kvault(s) => s.build_deposit_ix(wv, user, in_amount, min_out),
            Source::Save(s) => s.build_deposit_ix(wv, user, in_amount, min_out),
            Source::Marginfi(s) => s.build_deposit_ix(wv, user, in_amount, min_out),
            Source::Lulo(s) => s.build_deposit_ix(wv, user, in_amount, min_out),
        }
    }

    pub fn build_withdraw_ix(
        &self,
        wv: &WrapperVault,
        user: Pubkey,
        in_amount: u64,
        min_out: u64,
    ) -> Result<Instruction, TradingVenueError> {
        match self {
            Source::Klend(s) => s.build_withdraw_ix(wv, user, in_amount, min_out),
            Source::Kvault(s) => s.build_withdraw_ix(wv, user, in_amount, min_out),
            Source::Save(s) => s.build_withdraw_ix(wv, user, in_amount, min_out),
            Source::Marginfi(s) => s.build_withdraw_ix(wv, user, in_amount, min_out),
            Source::Lulo(s) => s.build_withdraw_ix(wv, user, in_amount, min_out),
        }
    }

    pub fn lookup_table_keys(&self, wv: &WrapperVault) -> Vec<Pubkey> {
        match self {
            Source::Klend(s) => s.lookup_table_keys(wv),
            Source::Kvault(s) => s.lookup_table_keys(wv),
            Source::Save(s) => s.lookup_table_keys(wv),
            Source::Marginfi(s) => s.lookup_table_keys(wv),
            Source::Lulo(s) => s.lookup_table_keys(wv),
        }
    }
}

fn make_source(protocol: ProtocolId) -> Source {
    match protocol {
        ProtocolId::Klend => Source::Klend(Klend::new()),
        ProtocolId::Kvault => Source::Kvault(Kvault::new()),
        ProtocolId::Save => Source::Save(Save::new()),
        ProtocolId::Marginfi => Source::Marginfi(Marginfi::new()),
        ProtocolId::Lulo => Source::Lulo(Lulo::new()),
    }
}

const PRICE_FLOOR: u64 = 100_000;

fn price_parts(
    bounds: Option<(u64, u64)>,
    quote: impl Fn(u64) -> Option<u64>,
) -> Vec<(u64, f64)> {
    let Some((lb, ub)) = bounds else {
        return Vec::new();
    };
    if ub <= lb {
        return Vec::new();
    }
    let mut edges = vec![ub];
    let mut c = ub;
    while c / 2 > PRICE_FLOOR && c / 2 > lb {
        c /= 2;
        edges.push(c);
    }
    edges.push(lb);
    edges.sort_unstable();
    edges.dedup();

    let mut parts: Vec<(u64, f64)> = Vec::new();
    for w in edges.windows(2) {
        let (a, b) = (w[0], w[1]);
        if let (Some(oa), Some(ob)) = (quote(a), quote(b)) {
            if ob > oa {
                parts.push((a, (ob - oa) as f64 / (b - a) as f64));
            }
        }
    }
    for i in (0..parts.len().saturating_sub(1)).rev() {
        let next = parts[i + 1].1;
        if parts[i].1 < next {
            parts[i].1 = next;
        }
    }
    parts
}

impl FromAccount for OverpassVenue {
    fn from_account(pubkey: &Pubkey, account: &Account) -> Result<Self, TradingVenueError> {
        let wv = state::decode_wrapper_vault(&account.data)?;
        let protocol_id = ProtocolId::from_byte(wv.protocol)?;
        let source = make_source(protocol_id);
        Ok(Self {
            wrapper_vault_pda: *pubkey,
            wrapper_vault: Some(wv),
            global_config: None,
            token_info: [TokenInfo::default(); 2],
            source: Some(source),
            parts_deposit: Vec::new(),
            parts_withdraw: Vec::new(),
        })
    }
}

#[async_trait]
impl TradingVenue for OverpassVenue {
    fn initialized(&self) -> bool {
        self.wrapper_vault.is_some()
            && self.global_config.is_some()
            && self.source.is_some()
            && self.token_info[0].pubkey != Pubkey::default()
            && !matches!(self.source, Some(Source::Lulo(_)))
    }

    fn program_id(&self) -> Pubkey {
        OVERPASS_PROGRAM_ID
    }

    fn program_dependencies(&self) -> Vec<Pubkey> {
        let mut deps = vec![OVERPASS_PROGRAM_ID];
        match &self.source {
            Some(Source::Klend(_)) => deps.push(klend::KLEND_PROGRAM_ID),
            Some(Source::Kvault(_)) => {
                deps.push(kvault::KVAULT_PROGRAM_ID);
                deps.push(klend::KLEND_PROGRAM_ID);
                deps.push(kvault::FARMS_PROGRAM_ID);
            }
            Some(Source::Save(_)) => deps.push(save::SAVE_PROGRAM_ID),
            Some(Source::Marginfi(_)) => deps.push(marginfi::MARGINFI_PROGRAM_ID),
            Some(Source::Lulo(_)) => deps.push(lulo::LULO_PROGRAM_ID),
            None => {}
        }
        for info in &self.token_info {
            let program = info.get_token_program();
            if !deps.contains(&program) {
                deps.push(program);
            }
        }
        deps
    }

    fn market_id(&self) -> Pubkey {
        self.wrapper_vault_pda
    }

    fn get_token_info(&self) -> &[TokenInfo] {
        &self.token_info
    }

    fn protocol(&self) -> PoolProtocol {
        PoolProtocol::Overpass
    }

    fn directions_num(&self) -> Vec<(u8, u8)> {
        let supply = self
            .wrapper_vault
            .as_ref()
            .map(|w| w.wrapper_supply)
            .unwrap_or(0);
        let deposit_ok = self.bounds(0, 1).is_ok();
        let withdraw_ok = self.bounds(1, 0).is_ok();
        if supply > 0 && !withdraw_ok {
            return Vec::new();
        }
        let mut dirs = Vec::new();
        if deposit_ok {
            dirs.push((0, 1));
        }
        if withdraw_ok {
            dirs.push((1, 0));
        }
        dirs
    }

    fn get_required_pubkeys_for_update(&self) -> Result<Vec<Pubkey>, TradingVenueError> {
        let wv = self
            .wrapper_vault
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("wrapper_vault".into()))?;
        let source = self
            .source
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("source".into()))?;
        let mut keys = vec![
            self.wrapper_vault_pda,
            state::global_config_pda(&OVERPASS_PROGRAM_ID),
            wv.underlying_mint,
            wv.wrapper_mint,
        ];
        keys.extend(source.required_pubkeys(wv));
        Ok(keys)
    }

    async fn update_state(&mut self, cache: &dyn AccountsCache) -> Result<(), TradingVenueError> {
        let wv_pda = self.wrapper_vault_pda;
        let gc_pda = state::global_config_pda(&OVERPASS_PROGRAM_ID);

        let wv_acct = cache
            .get_account(&wv_pda)
            .await?
            .ok_or(TradingVenueError::NoAccountFound(wv_pda.into()))?;
        let wv = state::decode_wrapper_vault(&wv_acct.data)?;

        let accounts = cache
            .get_accounts(&[gc_pda, wv.underlying_mint, wv.wrapper_mint])
            .await?;
        let [gc_opt, underlying_mint_opt, wrapper_mint_opt]: [Option<Account>; 3] = accounts
            .try_into()
            .map_err(|_| TradingVenueError::FailedToFetchMultipleAccountData)?;

        let gc_acct = gc_opt.ok_or(TradingVenueError::NoAccountFound(gc_pda.into()))?;
        let gc = state::decode_global_config(&gc_acct.data)?;

        let underlying_mint_acct = underlying_mint_opt
            .ok_or(TradingVenueError::NoAccountFound(wv.underlying_mint.into()))?;
        let wrapper_mint_acct = wrapper_mint_opt
            .ok_or(TradingVenueError::NoAccountFound(wv.wrapper_mint.into()))?;

        self.token_info[0] = TokenInfo::new(&wv.underlying_mint, &underlying_mint_acct, u64::MAX)?;
        self.token_info[1] = TokenInfo::new(&wv.wrapper_mint, &wrapper_mint_acct, u64::MAX)?;

        let current_protocol = ProtocolId::from_byte(wv.protocol)?;
        let needs_rebuild = !matches!(
            (&self.source, current_protocol),
            (Some(Source::Klend(_)), ProtocolId::Klend)
                | (Some(Source::Kvault(_)), ProtocolId::Kvault)
                | (Some(Source::Save(_)), ProtocolId::Save)
                | (Some(Source::Marginfi(_)), ProtocolId::Marginfi)
                | (Some(Source::Lulo(_)), ProtocolId::Lulo)
        );
        if needs_rebuild {
            self.source = Some(make_source(current_protocol));
        }

        let source = self
            .source
            .as_mut()
            .ok_or(TradingVenueError::NotInitialized("source".into()))?;
        source.update(&wv, cache).await?;

        self.wrapper_vault = Some(wv);
        self.global_config = Some(gc);

        let bd = self.bounds(0, 1).ok();
        let bw = self.bounds(1, 0).ok();
        let (pd, pw) = match (
            self.source.as_ref(),
            self.wrapper_vault.as_ref(),
            self.global_config.as_ref(),
        ) {
            (Some(source), Some(wv), Some(gc)) => (
                price_parts(bd, |x| source.quote_deposit(wv, gc, x).ok()),
                price_parts(bw, |x| source.quote_withdraw(wv, x).ok()),
            ),
            _ => (Vec::new(), Vec::new()),
        };
        self.parts_deposit = pd;
        self.parts_withdraw = pw;
        Ok(())
    }

    fn quote(&self, request: QuoteRequest) -> Result<QuoteResult, TradingVenueError> {
        if matches!(self.source, Some(Source::Lulo(_))) {
            return Err(TradingVenueError::UnsupportedVenue("lulo".into()));
        }
        if request.swap_type == SwapType::ExactOut {
            return Err(TradingVenueError::ExactOutNotSupported);
        }
        let wv = self
            .wrapper_vault
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("wrapper_vault".into()))?;
        let gc = self
            .global_config
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("global_config".into()))?;
        let source = self
            .source
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("source".into()))?;
        let dir = direction::detect(wv, request.input_mint, request.output_mint)?;
        let parts = match dir {
            Direction::Deposit => &self.parts_deposit,
            Direction::Withdraw => &self.parts_withdraw,
        };
        let price = parts
            .iter()
            .rev()
            .find(|(c, _)| request.amount >= *c)
            .map(|(_, p)| *p)
            .or_else(|| parts.first().map(|(_, p)| *p))
            .unwrap_or(0.0);
        if request.amount >= u64::MAX / 2 {
            return Ok(QuoteResult {
                input_mint: request.input_mint,
                output_mint: request.output_mint,
                amount: 0,
                expected_output: 0,
                not_enough_liquidity: true,
                price,
            });
        }
        let out = match dir {
            Direction::Deposit => source.quote_deposit(wv, gc, request.amount),
            Direction::Withdraw => source.quote_withdraw(wv, request.amount),
        };
        let (amount, expected_output, not_enough_liquidity) = match out {
            Ok(v) => (request.amount, v, false),
            Err(_) => (0, 0, true),
        };
        Ok(QuoteResult {
            input_mint: request.input_mint,
            output_mint: request.output_mint,
            amount,
            expected_output,
            not_enough_liquidity,
            price,
        })
    }

    fn generate_swap_instruction(
        &self,
        request: QuoteRequest,
        user: Pubkey,
    ) -> Result<Instruction, TradingVenueError> {
        if request.swap_type == SwapType::ExactOut {
            return Err(TradingVenueError::ExactOutNotSupported);
        }
        let wv = self
            .wrapper_vault
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("wrapper_vault".into()))?;
        let source = self
            .source
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("source".into()))?;
        let dir = direction::detect(wv, request.input_mint, request.output_mint)?;
        match dir {
            Direction::Deposit => source.build_deposit_ix(wv, user, request.amount, 0),
            Direction::Withdraw => source.build_withdraw_ix(wv, user, request.amount, 0),
        }
    }
}

#[async_trait]
impl AddressLookupTableTrait for OverpassVenue {
    async fn get_lookup_table_keys(
        &self,
        _accounts_cache: Option<&dyn AccountsCache>,
    ) -> Result<Vec<Pubkey>, TradingVenueError> {
        let wv = self
            .wrapper_vault
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("wrapper_vault".into()))?;
        let source = self
            .source
            .as_ref()
            .ok_or(TradingVenueError::NotInitialized("source".into()))?;
        Ok(source.lookup_table_keys(wv))
    }
}
