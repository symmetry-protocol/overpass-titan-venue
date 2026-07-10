//! Overpass wrapper-creation parsing test.

use solana_pubkey::{Pubkey, pubkey};

use titan_integration_template::your_venue::{OVERPASS_PROGRAM_ID, parse_pool_creations};
use titan_integration_template::trading_venue::protocol::PoolProtocol;
use titan_integration_template::trading_venue::venue_creation::{ParsedInstruction, PoolCreation};

const POOL: Pubkey = pubkey!("9iJeZPrNJrBEjj4h7tD9P3hygyM4hpZmyqrQmHjNUpdA");
const TOKEN_A_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
const TOKEN_B_MINT: Pubkey = pubkey!("HtnnuzVtecjktsUoZjHaZoZ9u4jPAzGpujE82U3v3cAV");

fn your_venue_pool_creation() -> ParsedInstruction {
    let mut data = vec![136, 245, 198, 59, 16, 197, 150, 217];
    data.extend_from_slice(&[0u8; 8]);

    let mut accounts = vec![Pubkey::new_unique(); 20];
    accounts[3] = TOKEN_B_MINT;
    accounts[4] = POOL;
    accounts[7] = TOKEN_A_MINT;

    ParsedInstruction {
        program_id: OVERPASS_PROGRAM_ID,
        accounts,
        data,
    }
}

fn unrelated_instruction() -> ParsedInstruction {
    ParsedInstruction {
        program_id: OVERPASS_PROGRAM_ID,
        accounts: vec![],
        data: vec![],
    }
}

#[test]
fn parses_your_venue_pool_creation() {
    let creations = parse_pool_creations(&[your_venue_pool_creation()]);

    assert_eq!(
        creations,
        vec![PoolCreation {
            protocol: PoolProtocol::Overpass,
            pool: POOL,
            mints: vec![TOKEN_A_MINT, TOKEN_B_MINT],
        }],
    );
}

#[test]
fn ignores_transactions_without_a_creation() {
    let creations = parse_pool_creations(&[unrelated_instruction()]);
    assert!(
        creations.is_empty(),
        "a transaction without a pool creation creates no pools, got {creations:?}"
    );
}
