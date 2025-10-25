use anchor_lang::prelude::*;

declare_id!("4GvwyPdEK3PKGoUAhBbLmAgmwgEBi8UqQmEimm7d6Hzg");

#[program]
pub mod mock_pyth {
    use super::*;

    pub fn set_feed(
        ctx: Context<SetFeed>,
        price: i64,
        conf: u64,
        expo: i32,
        publish_time: i64,
        ema_price: i64,
        ema_conf: u64,
        slot: u64,
        proof_available_time: u64,
        prev_publish_time: i64,
    ) -> Result<()> {
        let price_account = &mut ctx.accounts.price_account;
        
        // Set price data
        price_account.price = price;
        price_account.conf = conf;
        price_account.expo = expo;
        price_account.publish_time = publish_time;
        
        // Set EMA price data
        price_account.ema_price = ema_price;
        price_account.ema_conf = ema_conf;
        price_account.ema_expo = expo; // Same expo as regular price
        price_account.ema_publish_time = publish_time;
        
        // Set metadata
        price_account.slot = slot;
        price_account.proof_available_time = proof_available_time;
        price_account.prev_publish_time = prev_publish_time;
        
        Ok(())
    }
}

#[derive(Accounts)]
pub struct SetFeed<'info> {
    #[account(
        init_if_needed,
        payer = authority,
        space = 1024,
        seeds = [b"mock_v1"],
        bump
    )]
    pub price_account: Account<'info, PriceAccount>,
    
    #[account(mut)]
    pub authority: Signer<'info>,
    
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(Default)]
pub struct PriceAccount {
    // Price data
    pub price: i64,
    pub conf: u64,
    pub expo: i32,
    pub publish_time: i64,
    
    // EMA price data
    pub ema_price: i64,
    pub ema_conf: u64,
    pub ema_expo: i32,
    pub ema_publish_time: i64,
    
    // Metadata
    pub slot: u64,
    pub proof_available_time: u64,
    pub prev_publish_time: i64,
}

