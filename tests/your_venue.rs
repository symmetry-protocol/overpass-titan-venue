mod common;

use std::env;
use std::str::FromStr;

use common::SuiteConfig;
use solana_pubkey::{Pubkey, pubkey};
use titan_integration_template::your_venue::{OVERPASS_PROGRAM_ID, OverpassVenue};

#[cfg(debug_assertions)]
#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const OVERPASS_PROGRAMS: [Pubkey; 7] = [
    OVERPASS_PROGRAM_ID,
    pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD"),
    pubkey!("KvauGMspG5k6rtzrqqn7WNn3oZdyKqLKwK2XWQ8FLjd"),
    pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr"),
    pubkey!("So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo"),
    pubkey!("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA"),
    pubkey!("FL3X2pRsQ9zHENpZSKDRREtccwJuei8yg9fwDu9UN69Q"),
];

fn pool() -> Pubkey {
    env::var("WRAPPER_VAULT")
        .ok()
        .and_then(|s| Pubkey::from_str(&s).ok())
        .unwrap_or(pubkey!("9iJeZPrNJrBEjj4h7tD9P3hygyM4hpZmyqrQmHjNUpdA"))
}

fn programs() -> Vec<Pubkey> {
    OVERPASS_PROGRAMS.to_vec()
}

fn config() -> SuiteConfig {
    SuiteConfig {
        pool: pool(),
        programs: programs(),
    }
}

#[tokio::test]
async fn construction() {
    common::construction::<OverpassVenue>(&config()).await;
}

#[tokio::test]
async fn zero_input_spot_price() {
    common::zero_input_spot_price::<OverpassVenue>(&config()).await;
}

#[tokio::test]
async fn bound_simulation() {
    common::bound_simulation::<OverpassVenue>(&config()).await;
}

#[tokio::test]
async fn random_samples() {
    common::random_samples::<OverpassVenue>(&config()).await;
}

#[tokio::test]
async fn monotone() {
    common::monotone::<OverpassVenue>(&config()).await;
}

#[tokio::test]
async fn quoting_speed() {
    common::quoting_speed::<OverpassVenue>(&config()).await;
}

#[tokio::test]
async fn price_monotone() {
    common::price_monotone::<OverpassVenue>(&config()).await;
}

#[tokio::test]
async fn mean_value_theorem() {
    common::mean_value_theorem::<OverpassVenue>(&config()).await;
}
