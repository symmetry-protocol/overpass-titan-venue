//! Overpass swap-route test — the shared end-to-end route suite run against
//! `OverpassVenue`. SKIPs unless SOLANA_RPC_URL is set and the route program is
//! built.

mod common;

use std::env;
use std::str::FromStr;

use common::{RouteConfig, run_swap_route};
use solana_pubkey::{Pubkey, pubkey};
use titan_integration_template::your_venue::OverpassVenue;

fn pool() -> Pubkey {
    env::var("WRAPPER_VAULT")
        .ok()
        .and_then(|s| Pubkey::from_str(&s).ok())
        .unwrap_or(pubkey!("H7EovyxTZBMFNRkJJtMUVzAzs5uEzZBxj9RpUUKbjFmU"))
}

fn venue_programs() -> Vec<Pubkey> {
    vec![
        pubkey!("WRAPdXmxrH37RKUbH1QMnYrKdNe8w4Kz44t1cXmYeum"),
        pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD"),
        pubkey!("KvauGMspG5k6rtzrqqn7WNn3oZdyKqLKwK2XWQ8FLjd"),
        pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr"),
        pubkey!("So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo"),
        pubkey!("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"),
        pubkey!("FL3X2pRsQ9zHENpZSKDRREtccwJuei8yg9fwDu9UN69Q"),
    ]
}

#[tokio::test]
async fn swap_route_both_directions() {
    run_swap_route::<OverpassVenue>(RouteConfig {
        pool: pool(),
        venue_programs: venue_programs(),
    })
    .await;
}
