use anchor_lang::{prelude::*, solana_program::instruction::Instruction};

pub const PROGRAM_ID: Pubkey = pubkey!("WRAPdXmxrH37RKUbH1QMnYrKdNe8w4Kz44t1cXmYeum");

pub fn swap(
    discriminator: [u8; 8],
    amount_in: u64,
    account_metas: &[AccountMeta],
) -> Result<Vec<Instruction>> {
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&discriminator);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());

    Ok(vec![Instruction {
        program_id: PROGRAM_ID,
        accounts: account_metas.to_vec(),
        data,
    }])
}
