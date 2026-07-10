#[cfg(test)]
mod simulations_sweep {
    //! Quoting tests for Titan-compatible AMM venues.
    //!
    //! The tests ensure:
    //! - The venue loads on-chain state correctly
    //! - It exposes valid token info
    //! - It establishes valid quoting boundaries for both swap directions
    //! - Its off-chain quote matches on-chain execution on and off the boundaries
    //! - Its quoting speed is sufficient for integration
    //!
    //! Any AMM integrator must pass these quoting tests to ensure their pool
    //! is safe, consistent, and suitable for Titan routing.

    use litesvm::LiteSVM;
    use rand::Rng;
    use rstest::rstest;

    use solana_account::Account;
    use solana_account::ReadableAccount;
    use solana_account::WritableAccount;
    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_compute_budget::compute_budget::ComputeBudget;
    use solana_program::native_token::LAMPORTS_PER_SOL;
    use solana_program_pack::Pack;
    use solana_pubkey::{Pubkey, pubkey};
    use solana_sdk::signature::Keypair;
    use solana_sdk::signer::Signer;
    use solana_sysvar::clock::{self, Clock};
    use solana_transaction::Transaction;
    use std::str::FromStr;
    use std::time::Instant;

    use spl_associated_token_account::get_associated_token_address_with_program_id;
    use spl_token::state::{Account as TokenAccount, AccountState};

    use std::env;

    use titan_integration_template::your_venue::OVERPASS_PROGRAM_ID;
    use titan_integration_template::trading_venue::SwapType;

    use titan_integration_template::{
        account_caching::AccountsCache, your_venue::OverpassVenue, trading_venue::QuoteRequest,
    };
    use titan_integration_template::{
        account_caching::rpc_cache::RpcClientCache,
        trading_venue::{FromAccount, TradingVenue, error::TradingVenueError},
    };

    /// Every program the venue may CPI into during a swap. LiteSVM needs each
    /// loaded before the tx runs. Listed by pubkey → filename (`programs/<pk>.so`).
    const OVERPASS_PROGRAMS: &[Pubkey] = &[
        OVERPASS_PROGRAM_ID,
        pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD"),  // klend
        pubkey!("KvauGMspG5k6rtzrqqn7WNn3oZdyKqLKwK2XWQ8FLjd"),  // kvault (CPIs klend + farms)
        pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr"),  // farms (kvault dep)
        pubkey!("So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo"),  // save (Solend)
        pubkey!("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"),  // marginfi
        pubkey!("FL3X2pRsQ9zHENpZSKDRREtccwJuei8yg9fwDu9UN69Q"),  // lulo
    ];

    /// Initialize logging for test diagnostics.
    fn init_test_logger() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    /// Fetch a single account from the raw RPC client, retrying on HTTP 429.
    async fn rpc_get_with_retry(rpc: &RpcClient, pk: &Pubkey) -> Account {
        let mut backoff_ms: u64 = 2_000;
        for _ in 0..8 {
            match rpc.get_account(pk).await {
                Ok(a) => return a,
                Err(e) => {
                    let s = format!("{:?}", e);
                    if s.contains("429") || s.contains("Too Many Requests") {
                        std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                        backoff_ms = (backoff_ms * 2).min(10_000);
                        continue;
                    }
                    panic!("get_account {}: {}", pk, s);
                }
            }
        }
        panic!("get_account {} exhausted 429 retries", pk);
    }

    /// Run `venue.update_state`, retrying on HTTP 429 rate limits.
    async fn update_state_with_retry(venue: &mut OverpassVenue, cache: &RpcClientCache) {
        let mut backoff_ms: u64 = 2_000;
        for attempt in 0..8 {
            match venue.update_state(cache).await {
                Ok(()) => return,
                Err(e) => {
                    let s = format!("{:?}", e);
                    if (s.contains("429") || s.contains("Too Many Requests")) && attempt < 7 {
                        std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                        backoff_ms = (backoff_ms * 2).min(10_000);
                        continue;
                    }
                    panic!("update_state: {}", s);
                }
            }
        }
    }

    /// Fetch via the `RpcClientCache`, retrying on HTTP 429 rate limits.
    async fn cache_get_with_retry(cache: &RpcClientCache, pk: &Pubkey) -> Option<Account> {
        let mut backoff_ms: u64 = 2_000;
        for _ in 0..8 {
            match cache.get_account(pk).await {
                Ok(v) => return v,
                Err(e) => {
                    let s = format!("{:?}", e);
                    if s.contains("429") || s.contains("Too Many Requests") {
                        std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                        backoff_ms = (backoff_ms * 2).min(10_000);
                        continue;
                    }
                    panic!("cache get_account {}: {}", pk, s);
                }
            }
        }
        panic!("cache get_account {} exhausted 429 retries", pk);
    }

    /// Creates a new LiteSVM instance configured with:
    /// - Necessary helper programs loaded from `programs/`
    /// - A funded system account for signing transactions
    ///
    /// Integrators should update the programs loaded here for their own tests.
    pub fn setup_litesvm() -> (LiteSVM, Keypair) {
        let mut litesvm = LiteSVM::new()
            .with_compute_budget(ComputeBudget {
                compute_unit_limit: 1_400_000,
                ..Default::default()
            })
            .with_blockhash_check(false)
            .with_sigverify(false)
            .with_transaction_history(0);

        // Load every program our venue may CPI into during a swap.
        for program in OVERPASS_PROGRAMS {
            let path = format!("programs/{}.so", program);
            litesvm.add_program_from_file(*program, path).unwrap();
        }

        // Create a funded user wallet.
        let keypair = Keypair::new();
        let account = Account {
            lamports: 10_000 * LAMPORTS_PER_SOL,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };
        litesvm
            .set_account(keypair.pubkey(), account.into())
            .unwrap();

        (litesvm, keypair)
    }

    /// Simulate a swap using LiteSVM and return the output amount of token B.
    /// This should give the true on-chain output for that swap.
    /// Returns `Err(msg)` instead of panicking when the SVM rejects the tx, so
    /// the caller can aggregate per-sample stats.
    async fn sim_quote_request(
        venue: &dyn TradingVenue,
        cache: &dyn AccountsCache,
        request: QuoteRequest,
        litesvm: &mut LiteSVM,
        keypair: &Keypair,
    ) -> Result<u64, String> {
        let tradable_mints = venue.get_token_info();

        // Identify which token is A and which is B (depending on swap direction)
        let idx_0 = tradable_mints
            .iter()
            .position(|x| x.pubkey == request.input_mint)
            .unwrap();
        let idx_1 = (idx_0 + 1) % 2;

        let (token_a, token_a_program) = (
            tradable_mints[idx_0].pubkey,
            tradable_mints[idx_0].get_token_program(),
        );
        let (token_b, token_b_program) = (
            tradable_mints[idx_1].pubkey,
            tradable_mints[idx_1].get_token_program(),
        );

        let token_account_a = get_associated_token_address_with_program_id(
            &keypair.pubkey(),
            &token_a,
            &token_a_program,
        );
        let token_account_b = get_associated_token_address_with_program_id(
            &keypair.pubkey(),
            &token_b,
            &token_b_program,
        );

        //
        // Create synthetic token accounts inside the simulator
        //

        // Token A account (source). For wSOL (the native mint), maintain the
        // SPL Token native invariant: lamports = rent_exempt + amount, and
        // `is_native = Some(rent_exempt)`. Otherwise SPL Token sees inconsistent
        // state when our wrapper later does an inner native transfer.
        let native_mint = solana_pubkey::pubkey!("So11111111111111111111111111111111111111112");
        let is_wsol = token_a == native_mint;
        let rent_exempt: u64 = 2_039_280;
        let amount_a: u64 = u64::MAX / 2; // headroom for the deposit
        let lamports_a = if is_wsol { rent_exempt + amount_a } else { LAMPORTS_PER_SOL };

        let mut account_a = Account::new(lamports_a, TokenAccount::LEN, &token_a_program);
        let mut account_a_data = TokenAccount::default();
        account_a_data.mint = token_a;
        account_a_data.owner = keypair.pubkey();
        account_a_data.state = AccountState::Initialized;
        account_a_data.amount = if is_wsol { amount_a } else { u64::MAX };
        if is_wsol {
            account_a_data.is_native = spl_token::solana_program::program_option::COption::Some(rent_exempt);
        }
        account_a_data.pack_into_slice(account_a.data_as_mut_slice());

        // Token B account (destination)
        let mut account_b = Account::new(LAMPORTS_PER_SOL, TokenAccount::LEN, &token_b_program);
        let mut account_b_data = TokenAccount::default();
        account_b_data.mint = token_b;
        account_b_data.owner = keypair.pubkey();
        account_b_data.state = AccountState::Initialized;
        account_b_data.amount = 0;
        account_b_data.pack_into_slice(account_b.data_as_mut_slice());

        // Load accounts into LiteSVM
        litesvm.set_account(token_account_a, account_a).unwrap();
        litesvm.set_account(token_account_b, account_b).unwrap();

        //
        // Build the swap instruction
        //
        let ix = venue
            .generate_swap_instruction(request, keypair.pubkey())
            .unwrap();

        // Load all instruction accounts into SVM (except executable ones already present)
        let pks: Vec<Pubkey> = ix.accounts.iter().map(|acc| acc.pubkey).collect();
        let accounts_to_load = {
            let mut backoff_ms: u64 = 2_000;
            let mut out = None;
            for _ in 0..8 {
                match cache.get_accounts(&pks).await {
                    Ok(v) => {
                        out = Some(v);
                        break;
                    }
                    Err(e) => {
                        let s = format!("{:?}", e);
                        if s.contains("429") || s.contains("Too Many Requests") {
                            std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                            backoff_ms = (backoff_ms * 2).min(10_000);
                            continue;
                        }
                        return Err(format!("rpc fetch: {}", s));
                    }
                }
            }
            out.ok_or_else(|| "rpc fetch exhausted 429 retries".to_string())?
        };
        for (account, key) in accounts_to_load.iter().zip(pks) {
            if let Some(acc) = account {
                if acc.executable {
                    continue;
                }
                litesvm.set_account(key, acc.clone()).unwrap();
            }
        }

        //
        // Execute swap inside the SIM
        //
        let blockhash = litesvm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&keypair.pubkey()),
            &[keypair],
            blockhash,
        );

        let simulation_result = litesvm.simulate_transaction(tx).map_err(|e| {
            // Extract the deepest failing program from the log trail. Marginfi/klend/SPL
            // logs follow the pattern "Program <pubkey> failed: ...". We grab the last
            // such line so the caller knows which step blew up.
            let failing_program = e
                .meta
                .logs
                .iter()
                .rev()
                .find(|line| line.contains("failed:"))
                .cloned()
                .unwrap_or_else(|| "<no failed: line in logs>".to_string());
            // Find the most recent inner-program invocation context (e.g. the
            // CPI that failed) to know which step inside our wrapper broke.
            let last_invoke = e
                .meta
                .logs
                .iter()
                .rev()
                .find(|l| l.contains("invoke ["))
                .cloned()
                .unwrap_or_default();
            let last_ix_log = e
                .meta
                .logs
                .iter()
                .rev()
                .find(|l| l.starts_with("Program log: Instruction:"))
                .cloned()
                .unwrap_or_default();
            // TEMPORARY: experimental full-log dump, may be reverted.
            let all_logs = e.meta.logs.join("\n  ");
            format!(
                "svm reject: {:?} | at {} | last ix: {} | {}\n  --- FULL PROGRAM LOGS ---\n  {}",
                e.err, last_invoke, last_ix_log, failing_program, all_logs
            )
        })?;

        //
        // Read output account and extract the final token amount
        //
        let account_b = simulation_result
            .post_accounts
            .into_iter()
            .find(|(pk, _)| pk == &token_account_b)
            .map(|(_, acc)| acc)
            .ok_or_else(|| "no post-account for token_b".to_string())?;
        let post_b = TokenAccount::unpack_from_slice(account_b.data())
            .map_err(|e| format!("unpack token_b: {:?}", e))?;
        Ok(post_b.amount)
    }

    /// Returns a log-uniformly sampled u64 in `[lo, hi]`.
    fn sample_log_uniform_u64(lo: u64, hi: u64) -> u64 {
        assert!(lo >= 1, "log-uniform sampling requires lo >= 1");
        assert!(lo <= hi);

        let lo_f = lo as f64;
        let hi_f = hi as f64;

        let log_lo = lo_f.ln();
        let log_hi = hi_f.ln();

        let r: f64 = rand::rng().random();
        let log_val = log_lo + r * (log_hi - log_lo);

        (log_val.exp() as u64).clamp(lo, hi)
    }

    // -------------------------------------------------------------------------
    // Test 1: check boundary values in simulation
    // -------------------------------------------------------------------------

    #[rstest]
    #[tokio::test]
    #[case("B8jPKEcfjYt4LJKNMZfxmB116MfKmi5HZ5eMQtqcBTkz")]
    async fn test_bound_simulation(#[case] case_key: Pubkey) {
        init_test_logger();

        // Allow env-var override for cross-wrapper sweeps
        let amm_key = env::var("WRAPPER_VAULT")
            .ok()
            .and_then(|s| Pubkey::from_str(&s).ok())
            .unwrap_or(case_key);

        // Fetch live pool data from RPC
        let rpc_url = env::var("SOLANA_RPC_URL").unwrap();
        let rpc = RpcClient::new(rpc_url);
        let venue_account = rpc_get_with_retry(&rpc, &amm_key).await;

        // Build venue + load pool state
        let cache = RpcClientCache::new(rpc);
        let mut venue = OverpassVenue::from_account(&amm_key, &venue_account).unwrap();
        update_state_with_retry(&mut venue, &cache).await;

        // Setup simulation VM
        let (mut litesvm, keypair) = setup_litesvm();

        // Load Raydium AMM program binary

        // Sync sysvar clock to real network
        let latest_clock = cache_get_with_retry(&cache, &clock::ID).await;
        let mut latest_clock: Clock = latest_clock
            .as_ref()
            .ok_or(TradingVenueError::NoAccountFound(clock::ID.into()))
            .unwrap()
            .deserialize_data()
            .unwrap();

        // Lulo (protocol byte 11 == 3): flexlend recomputes its cached
        // protocol balance against the clock; the latest mainnet clock vs a
        // static snapshot makes it see phantom accrued interest and reject
        // every lulo CPI (BalanceStaleError, pool.rs:236). Pin the clock to
        // the lulo pool's last_updated (off 72) so flexlend sees 0 elapsed
        // and matches the off-chain quoter (which has no time term). No-op
        // for every other protocol (they need the live clock).
        if venue_account.data().len() >= 148 && venue_account.data()[11] == 3 {
            if let Ok(source_pool) =
                Pubkey::try_from(&venue_account.data()[116..148])
            {
                if let Some(pool) =
                    cache_get_with_retry(&cache, &source_pool).await
                {
                    let d = pool.data();
                    if d.len() >= 80 {
                        let last_updated = i64::from_le_bytes(
                            d[72..80].try_into().unwrap(),
                        );
                        let prev = latest_clock.unix_timestamp;
                        latest_clock.unix_timestamp = last_updated;
                        latest_clock.epoch_start_timestamp = last_updated;
                        log::info!(
                            "CLOCK-PIN(lulo) pool={} last_updated={} (was now={}, dt={}s)",
                            source_pool, last_updated, prev, prev - last_updated
                        );
                    }
                }
            }
        }

        litesvm.set_sysvar::<Clock>(&latest_clock);

        // Ensure valid token set
        let tradable_mints = venue.get_token_info();
        assert_eq!(tradable_mints.len(), 2);

        //
        // For each swap direction, verify that boundary quotes match simulation.
        //
        let mut matched = 0u32;
        let mut sim_errors = 0u32;
        let mut max_drift = 0u64;
        let mut total_bounds = 0u32;
        let mut bounds_errors = 0u32;
        for (in_idx, out_idx) in [(0, 1), (1, 0)] {
            let (lower, upper) = match venue.bounds(in_idx as u8, out_idx as u8) {
                Ok(b) => b,
                Err(e) => {
                    bounds_errors += 1;
                    log::info!(
                        "computed_bounds dir={}->{} ERR={:?}",
                        in_idx, out_idx, e
                    );
                    continue;
                }
            };
            log::info!(
                "computed_bounds dir={}->{} lo={} hi={}",
                in_idx, out_idx, lower, upper
            );

            for bound in [lower, upper] {
                total_bounds += 1;
                let request = QuoteRequest {
                    input_mint: venue.get_token(in_idx).unwrap().pubkey,
                    output_mint: venue.get_token(out_idx).unwrap().pubkey,
                    amount: bound,
                    swap_type: SwapType::ExactIn,
                };

                let sim_res =
                    sim_quote_request(&venue, &cache, request.clone(), &mut litesvm, &keypair)
                        .await;
                let quote = venue.quote(request).unwrap();

                match sim_res {
                    Ok(sim) => {
                        let delta = quote.expected_output.abs_diff(sim);
                        if delta == 0 {
                            matched += 1;
                        } else {
                            max_drift = max_drift.max(delta);
                        }
                        log::info!(
                            "bound={} sim={} quote={} delta={}",
                            bound, sim, quote.expected_output, delta
                        );
                    }
                    Err(e) => {
                        sim_errors += 1;
                        log::info!(
                            "bound={} quote={} sim_error={}",
                            bound, quote.expected_output, e
                        );
                    }
                }
            }
        }

        log::info!(
            "BOUND_SUMMARY: matched={}/{} sim_errors={} max_drift={} bounds_errors={}",
            matched, total_bounds, sim_errors, max_drift, bounds_errors
        );

        assert_eq!(
            matched, total_bounds,
            "bound_simulation: matched {}/{} (sim_errors={}, max_drift={}, bounds_errors={})",
            matched, total_bounds, sim_errors, max_drift, bounds_errors
        );
    }

    // -------------------------------------------------------------------------
    // Test 2: Random sampling simulation
    // -------------------------------------------------------------------------

    #[rstest]
    #[tokio::test]
    #[case("B8jPKEcfjYt4LJKNMZfxmB116MfKmi5HZ5eMQtqcBTkz")]
    async fn test_random_samples(#[case] case_key: Pubkey) {
        init_test_logger();

        // Allow env-var override for cross-wrapper sweeps
        let amm_key = env::var("WRAPPER_VAULT")
            .ok()
            .and_then(|s| Pubkey::from_str(&s).ok())
            .unwrap_or(case_key);

        // Fetch venue state from RPC
        let rpc_url = env::var("SOLANA_RPC_URL").unwrap();
        let rpc = RpcClient::new(rpc_url);
        let venue_account = rpc_get_with_retry(&rpc, &amm_key).await;

        let cache = RpcClientCache::new(rpc);
        let mut venue = OverpassVenue::from_account(&amm_key, &venue_account).unwrap();
        update_state_with_retry(&mut venue, &cache).await;

        // Setup simulation VM
        let (mut litesvm, keypair) = setup_litesvm();

        // Sync sysvar clock
        let latest_clock = cache_get_with_retry(&cache, &clock::ID).await;
        let latest_clock: Clock = latest_clock
            .as_ref()
            .ok_or(TradingVenueError::NoAccountFound(clock::ID.into()))
            .unwrap()
            .deserialize_data()
            .unwrap();
        litesvm.set_sysvar::<Clock>(&latest_clock);

        //
        // For each direction, randomly sample the entire valid quoting domain and
        // ensure that the quoted amount matches the simulated amount.
        //
        let mut total = 0u32;
        let mut matched = 0u32;
        let mut sim_errors = 0u32;
        let mut quote_errors = 0u32;
        let mut max_drift = 0u64;
        let mut bounds_errors = 0u32;
        for (in_idx, out_idx) in [(0, 1), (1, 0)] {
            let (computed_lo, computed_hi) = match venue.bounds(in_idx, out_idx) {
                Ok(b) => b,
                Err(e) => {
                    bounds_errors += 1;
                    log::info!(
                        "computed_bounds dir={}->{} ERR={:?}",
                        in_idx, out_idx, e
                    );
                    continue;
                }
            };
            log::info!(
                "computed_bounds dir={}->{} lo={} hi={}",
                in_idx, out_idx, computed_lo, computed_hi
            );
            let lb = computed_lo;
            let ub = computed_hi;
            log::info!(
                "test_bounds dir={}->{} lb={} ub={}",
                in_idx, out_idx, lb, ub
            );

            for _ in 0..50 {
                total += 1;
                let amount = sample_log_uniform_u64(lb, ub);

                let request = QuoteRequest {
                    input_mint: venue.get_token(in_idx as usize).unwrap().pubkey,
                    output_mint: venue.get_token(out_idx as usize).unwrap().pubkey,
                    amount,
                    swap_type: SwapType::ExactIn,
                };

                let quote_res = venue.quote(request.clone());
                let sim_res =
                    sim_quote_request(&venue, &cache, request, &mut litesvm, &keypair).await;

                match (quote_res, sim_res) {
                    (Ok(quote), Ok(sim)) => {
                        let delta = quote.expected_output.abs_diff(sim);
                        if delta == 0 {
                            matched += 1;
                        } else {
                            max_drift = max_drift.max(delta);
                        }
                        log::info!(
                            "random dir={}->{} amount={} sim={} quote={} delta={}",
                            in_idx, out_idx, amount, sim, quote.expected_output, delta
                        );
                    }
                    (Err(e), _) => {
                        quote_errors += 1;
                        log::info!(
                            "random dir={}->{} amount={} quote_err={:?}",
                            in_idx, out_idx, amount, e
                        );
                    }
                    (Ok(quote), Err(e)) => {
                        sim_errors += 1;
                        log::info!(
                            "random dir={}->{} amount={} quote={} sim_err={}",
                            in_idx, out_idx, amount, quote.expected_output, e
                        );
                    }
                }
            }
        }

        log::info!(
            "RANDOM_SUMMARY: matched={}/{} sim_errors={} quote_errors={} max_drift={} bounds_errors={}",
            matched, total, sim_errors, quote_errors, max_drift, bounds_errors
        );

        assert_eq!(
            matched, total,
            "random_samples: matched {}/{} (sim_errors={}, max_drift={}, bounds_errors={})",
            matched, total, sim_errors, max_drift, bounds_errors
        );
    }

    // -------------------------------------------------------------------------
    // Test 3: AMM Monotonicity
    // -------------------------------------------------------------------------

    #[rstest]
    #[tokio::test]
    #[case("B8jPKEcfjYt4LJKNMZfxmB116MfKmi5HZ5eMQtqcBTkz")]
    async fn test_monotone(#[case] case_key: String) -> () {
        init_test_logger();

        //
        // Prepare inputs
        //
        // Allow env-var override for cross-wrapper sweeps
        let amm_key = env::var("WRAPPER_VAULT")
            .ok()
            .and_then(|s| Pubkey::from_str(&s).ok())
            .unwrap_or_else(|| Pubkey::from_str(&case_key).expect("Invalid test pubkey"));

        let rpc_url =
            env::var("SOLANA_RPC_URL").expect("SOLANA_RPC_URL must be set for integration tests");
        let rpc = RpcClient::new(rpc_url);

        //
        // Fetch the venue’s account and construct the venue
        //
        let venue_account = rpc_get_with_retry(&rpc, &amm_key).await;

        let mut venue = OverpassVenue::from_account(&amm_key, &venue_account)
            .expect("Failed to construct venue from account");

        //
        // Load on-chain state using the caching layer
        //
        let cache = RpcClientCache::new(rpc);
        update_state_with_retry(&mut venue, &cache).await;

        //
        // Validate token metadata
        //
        let token_info = venue.get_token_info();
        log::debug!("Loaded token info: {:#?}", token_info);

        // Raydium AMMs always have 2 tokens.
        assert_eq!(token_info.len(), 2);

        //
        // For each direction (token0 → token1, token1 → token0)
        // is monotone increasing.
        //
        let mut total = 0u32;
        let mut monotone_ok = 0u32;
        let mut violations = 0u32;
        let mut quote_errors = 0u32;
        let mut bounds_errors = 0u32;
        for (in_idx, out_idx) in [(0, 1), (1, 0)] {
            let (computed_lo, computed_hi) = match venue.bounds(in_idx, out_idx) {
                Ok(b) => b,
                Err(e) => {
                    bounds_errors += 1;
                    log::info!(
                        "computed_bounds dir={}->{} ERR={:?}",
                        in_idx, out_idx, e
                    );
                    continue;
                }
            };
            log::info!(
                "computed_bounds dir={}->{} lo={} hi={}",
                in_idx, out_idx, computed_lo, computed_hi
            );
            let lb = computed_lo;
            let ub = computed_hi;
            log::info!(
                "test_bounds dir={}->{} lb={} ub={}",
                in_idx, out_idx, lb, ub
            );
            let mut test_amounts = Vec::with_capacity(50);

            for _ in 0..50 {
                test_amounts.push(sample_log_uniform_u64(lb, ub));
            }
            test_amounts.sort();

            let mut prev = 0;
            for amount in test_amounts {
                total += 1;
                let input_mint = token_info[in_idx as usize].pubkey;
                let output_mint = token_info[out_idx as usize].pubkey;

                let result = match venue.quote(QuoteRequest {
                    input_mint,
                    output_mint,
                    amount,
                    swap_type: SwapType::ExactIn,
                }) {
                    Ok(r) => r,
                    Err(_) => {
                        quote_errors += 1;
                        continue;
                    }
                };

                if prev <= result.expected_output {
                    monotone_ok += 1;
                } else {
                    violations += 1;
                    log::info!(
                        "monotone VIOLATION dir={}->{} amount={} prev_out={} cur_out={}",
                        in_idx, out_idx, amount, prev, result.expected_output
                    );
                }
                log::info!(
                    "monotone dir={}->{} amount={} quote={} nel={}",
                    in_idx, out_idx, amount, result.expected_output, result.not_enough_liquidity
                );
                prev = result.expected_output;
            }
        }

        log::info!(
            "MONOTONE_SUMMARY: monotone={}/{} violations={} quote_errors={} bounds_errors={}",
            monotone_ok, total, violations, quote_errors, bounds_errors
        );

        assert_eq!(
            violations, 0,
            "monotone: {} violations of {} (quote_errors={}, bounds_errors={})",
            violations, total, quote_errors, bounds_errors
        );
    }

    // -------------------------------------------------------------------------
    // Test 4: Quoting speed
    // -------------------------------------------------------------------------

    #[rstest]
    #[tokio::test]
    #[case("B8jPKEcfjYt4LJKNMZfxmB116MfKmi5HZ5eMQtqcBTkz", 10_000)]
    async fn test_quoting_speed(#[case] case_key: String, #[case] iterations: usize) -> () {
        init_test_logger();

        //
        // Prepare inputs
        //
        // Allow env-var override for cross-wrapper sweeps
        let amm_key = env::var("WRAPPER_VAULT")
            .ok()
            .and_then(|s| Pubkey::from_str(&s).ok())
            .unwrap_or_else(|| Pubkey::from_str(&case_key).expect("Invalid test pubkey"));

        let rpc_url =
            env::var("SOLANA_RPC_URL").expect("SOLANA_RPC_URL must be set for integration tests");
        let rpc = RpcClient::new(rpc_url);

        //
        // Fetch the venue’s account and construct the venue
        //
        let venue_account = rpc_get_with_retry(&rpc, &amm_key).await;

        let mut venue = OverpassVenue::from_account(&amm_key, &venue_account)
            .expect("Failed to construct venue from account");

        //
        // Load on-chain state using the caching layer
        //
        let cache = RpcClientCache::new(rpc);
        update_state_with_retry(&mut venue, &cache).await;

        //
        // Validate token metadata
        //
        let token_info = venue.get_token_info();
        log::debug!("Loaded token info: {:#?}", token_info);

        // Raydium AMMs always have 2 tokens.
        assert_eq!(token_info.len(), 2);

        //
        // For each direction (token0 → token1, token1 → token0)
        // verify quoting speed requirements are met.
        //
        let mut bounds_errors = 0u32;
        let mut directions_tested = 0u32;
        for (in_idx, out_idx) in [(0, 1), (1, 0)] {
            let input_mint = token_info[in_idx as usize].pubkey;
            let output_mint = token_info[out_idx as usize].pubkey;

            let (computed_lo, computed_hi) = match venue.bounds(in_idx, out_idx) {
                Ok(b) => b,
                Err(e) => {
                    bounds_errors += 1;
                    log::info!(
                        "computed_bounds dir={}->{} ERR={:?}",
                        in_idx, out_idx, e
                    );
                    continue;
                }
            };
            log::info!(
                "computed_bounds dir={}->{} lo={} hi={}",
                in_idx, out_idx, computed_lo, computed_hi
            );
            let lb = computed_lo;
            let ub = computed_hi;
            directions_tested += 1;
            let mut test_amounts = Vec::with_capacity(iterations);

            for _ in 0..iterations {
                test_amounts.push(sample_log_uniform_u64(lb, ub));
            }

            let start = Instant::now();
            let mut completed_iters: usize = 0;
            for amount in test_amounts {
                let res = venue.quote(QuoteRequest {
                    input_mint,
                    output_mint,
                    amount,
                    swap_type: SwapType::ExactIn,
                });
                if res.is_ok() {
                    completed_iters += 1;
                }
            }
            let elapsed = start.elapsed().as_secs_f64();
            let avg_time = if completed_iters > 0 {
                elapsed / completed_iters as f64
            } else {
                f64::INFINITY
            };

            log::info!(
                "SPEED_SUMMARY: dir={}->{} iters={} avg_secs={:.9}",
                in_idx, out_idx, iterations, avg_time
            );

            assert!(
                avg_time < 0.0001,
                "Failed quoting speed test swapping ({}) -> ({})",
                input_mint,
                output_mint
            );
        }
        log::info!(
            "SPEED_FINAL: directions_tested={} bounds_errors={}",
            directions_tested, bounds_errors
        );
        assert!(
            directions_tested > 0,
            "speed: no direction had usable bounds ({} bounds errors)",
            bounds_errors
        );
    }

}
