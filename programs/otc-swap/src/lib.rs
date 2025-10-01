// Stops Rust Analyzer complaining about missing configs
// See https://solana.stackexchange.com/questions/17777
#![allow(unexpected_cfgs)]

// Fix warning: use of deprecated method `anchor_lang::prelude::AccountInfo::<'a>::realloc`: Use AccountInfo::resize() instead
// See https://solana.stackexchange.com/questions/22979
#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_spl::token::{self, SetAuthority, Mint, Token, TokenAccount, Transfer, MintTo, Burn};
use spl_token::instruction::AuthorityType;


declare_id!("BkCS4J3v38Pfr5U37qXqdGfJr7YDHKrKutdctJg1caxv");

#[program]
pub mod otc_swap {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, fee_rate_bps: u64, min_collateral_bps: u64) -> Result<()> {
        // VALIDATION
        require!(fee_rate_bps <= 500, ErrorCode::InvalidFeeRate); // Max 5%
        require!(min_collateral_bps >= 1000, ErrorCode::InvalidCollateralRatio); // Min 10%
        require!(
            ctx.accounts.sbtc_mint.mint_authority == Some(ctx.accounts.squad_multisig.key()).into(),
            ErrorCode::InvalidMintAuthority
        );
        
        // TRANSFER MINT AUTHORITY
        let cpi_accounts = SetAuthority {
            current_authority: ctx.accounts.squad_multisig.to_account_info(),
            account_or_mint: ctx.accounts.sbtc_mint.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::set_authority(
            cpi_ctx, 
            AuthorityType::MintTokens, 
            Some(ctx.accounts.sbtc_mint_authority_pda.key())
        )?;
        
        // STORE CONFIG
        let config = &mut ctx.accounts.config;
        config.squad_multisig = ctx.accounts.squad_multisig.key();
        config.sbtc_mint = ctx.accounts.sbtc_mint.key();
        config.zbtc_mint = ctx.accounts.zbtc_mint.key();
        config.treasury_zbtc_vault = ctx.accounts.treasury_zbtc_vault.key();
        config.fee_vault = ctx.accounts.fee_vault.key();
        config.sbtc_mint_authority_pda = ctx.accounts.sbtc_mint_authority_pda.key();
        config.fee_rate_bps = fee_rate_bps;
        config.min_collateral_bps = min_collateral_bps;
        config.bump = ctx.bumps.config;
        
        // EMIT EVENT
        emit!(InitializedEvent {
            squad_multisig: ctx.accounts.squad_multisig.key(),
            sbtc_mint: ctx.accounts.sbtc_mint.key(),
            zbtc_mint: ctx.accounts.zbtc_mint.key(),
            treasury_vault: ctx.accounts.treasury_zbtc_vault.key(),
            fee_vault: ctx.accounts.fee_vault.key(),
            fee_rate_bps,
            min_collateral_bps,
            timestamp: Clock::get()?.unix_timestamp,
        });
        
        Ok(())
    }

    pub fn mint_sbtc(ctx: Context<MintSbtc>, zbtc_amount: u64) -> Result<()> {
        // VALIDATION
        require!(zbtc_amount > 0, ErrorCode::InvalidAmount);
        
        // Load config
        let config = &ctx.accounts.config;
        
        // Verify correct mints are used
        require!(
            ctx.accounts.zbtc_mint.key() == config.zbtc_mint,
            ErrorCode::InvalidZbtcMint
        );
        require!(
            ctx.accounts.sbtc_mint.key() == config.sbtc_mint,
            ErrorCode::InvalidSbtcMint
        );
        
        // Verify user owns the token accounts
        require!(
            ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(),
            ErrorCode::InvalidTokenAccountOwner
        );
        require!(
            ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(),
            ErrorCode::InvalidTokenAccountOwner
        );
        
        // Verify user has sufficient zBTC balance
        require!(
            ctx.accounts.user_zbtc_account.amount >= zbtc_amount,
            ErrorCode::InsufficientBalance
        );

        // Calculate fee and net amount
        let fee = zbtc_amount
            .checked_mul(config.fee_rate_bps)
            .unwrap()
            .checked_div(10000)
            .unwrap();
        
        let net_zbtc_amount = zbtc_amount.checked_sub(fee).unwrap();
        
        // Get sBTC price from oracle (currently hardcoded 1:1)
        let sbtc_price = get_sbtc_price()?; // Returns 1.0 for now
        let sbtc_to_mint = (net_zbtc_amount as f64 * sbtc_price) as u64;
        
        require!(sbtc_to_mint > 0, ErrorCode::InvalidAmount);

        // Transfer zBTC to treasury (net amount)
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_zbtc_account.to_account_info(),
                    to: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                }
            ),
            net_zbtc_amount,
        )?;

        // Transfer fee to fee vault
        if fee > 0 {
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.user_zbtc_account.to_account_info(),
                        to: ctx.accounts.fee_vault.to_account_info(),
                        authority: ctx.accounts.user.to_account_info(),
                    }
                ),
                fee,
            )?;
        }

        let seeds: &[&[u8]] = &[
            b"sbtc_mint_authority",
            &[ctx.bumps.sbtc_mint_authority_pda], // bump must be &[u8]
        ];
        let signer_seeds = &[seeds];       
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.sbtc_mint.to_account_info(),
                    to: ctx.accounts.user_sbtc_account.to_account_info(),
                    authority: ctx.accounts.sbtc_mint_authority_pda.to_account_info(),
                },
                signer_seeds,
            ),
            sbtc_to_mint,
        )?;

        // EMIT EVENT
        emit!(MintEvent {
            user: ctx.accounts.user.key(),
            zbtc_amount,
            sbtc_minted: sbtc_to_mint,
            fee_amount: fee,
            timestamp: Clock::get()?.unix_timestamp,
        });
        
        Ok(())
    }

    pub fn burn_sbtc(ctx: Context<BurnSbtc>, sbtc_amount: u64) -> Result<()> {
        // VALIDATION
        require!(sbtc_amount > 1000, ErrorCode::InvalidAmount); // amount > 1e-6 SOL (?)

        let config = &ctx.accounts.config;

        // Verify correct mints are used
        require!(
            ctx.accounts.zbtc_mint.key() == config.zbtc_mint,
            ErrorCode::InvalidZbtcMint
        );
        require!(
            ctx.accounts.sbtc_mint.key() == config.sbtc_mint,
            ErrorCode::InvalidSbtcMint
        );

        // Verify user owns the token accounts
        require!(
            ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(),
            ErrorCode::InvalidTokenAccountOwner
        );
        require!(
            ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(),
            ErrorCode::InvalidTokenAccountOwner
        );

        // Verify user has enough sBTC to burn
        require!(
            ctx.accounts.user_sbtc_account.amount >= sbtc_amount,
            ErrorCode::InsufficientBalance
        );

        // Fetch sBTC price (hardcoded 1:1 for now)
        let sbtc_price = get_sbtc_price()?; // returns f64 = 1.0

        // Compute equivalent zBTC value
        let zbtc_value = (sbtc_amount as f64 * sbtc_price) as u64;

        // Calculate fee and net redemption
        let fee = zbtc_value
            .checked_mul(config.fee_rate_bps)
            .unwrap()
            .checked_div(10_000)
            .unwrap();

        let net_zbtc = zbtc_value.checked_sub(fee).unwrap();

        require!(net_zbtc > 0, ErrorCode::InvalidAmount);

        // Verify treasury has enough liquidity
        require!(
            ctx.accounts.treasury_zbtc_vault.amount >= zbtc_value,
            ErrorCode::InsufficientBalance
        );

        // Burn sBTC from user
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.sbtc_mint.to_account_info(),
                    from: ctx.accounts.user_sbtc_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            sbtc_amount,
        )?;

        // Transfer net zBTC to user
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                    to: ctx.accounts.user_zbtc_account.to_account_info(),
                    authority: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                },
                &[&[b"treasury", config.squad_multisig.as_ref(), &[ctx.bumps.treasury_zbtc_vault]]],
            ),
            net_zbtc,
        )?;

        // Retain fee inside treasury: instead of moving it, we just don’t transfer it out.
        // But if you want to explicitly send it to fee_vault (tests suggest fees accumulate separately):
        if fee > 0 {
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                        to: ctx.accounts.fee_vault.to_account_info(),
                        authority: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                    },
                    &[&[b"treasury", config.squad_multisig.as_ref(), &[ctx.bumps.treasury_zbtc_vault]]],
                ),
                fee,
            )?;
        }

        // Emit event
        emit!(BurnEvent {
            user: ctx.accounts.user.key(),
            sbtc_burned: sbtc_amount,
            zbtc_redeemed: net_zbtc,
            fee_amount: fee,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}


// ========================= Utils Functions ================================
fn get_sbtc_price() -> Result<f64> {
    // TODO: Replace with actual oracle CPI call
    // For now, return hardcoded 1:1 price
    Ok(1.0)
}

// ========================= Accounts / PDAs ================================
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub squad_multisig: Signer<'info>,
    
    // sBTC mint (pre-created, authority will be transferred)
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,
    
    // zBTC mint (already exists)
    pub zbtc_mint: Account<'info, Mint>,
    
    // PDA that will become sBTC mint authority
    /// CHECK: PDA for sBTC mint authority
    #[account(seeds = [b"sbtc_mint_authority"], bump)]
    pub sbtc_mint_authority_pda: UncheckedAccount<'info>,
    
    // Treasury vault - PDA derived from squad_multisig
    #[account(
        init,
        payer = squad_multisig,
        token::mint = zbtc_mint,
        token::authority = treasury_zbtc_vault, // The PDA itself is the authority
        seeds = [b"treasury", squad_multisig.key().as_ref()],
        bump
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,
    
    // Fee vault - PDA derived from squad_multisig
    #[account(
        init,
        payer = squad_multisig,
        token::mint = zbtc_mint,
        token::authority = fee_vault, // The PDA itself is the authority
        seeds = [b"fees", squad_multisig.key().as_ref()],
        bump
    )]
    pub fee_vault: Account<'info, TokenAccount>,
    
    // Config PDA
    #[account(
        init,
        payer = squad_multisig,
        space = 8 + Config::INIT_SPACE,
        seeds = [b"config", squad_multisig.key().as_ref()],
        bump
    )]
    pub config: Account<'info, Config>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintSbtc<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// the multisig used as the seed for config/treasury/fees (does not need to be a signer here)
    /// supply the same squad_multisig that was used during initialize
    /// CHECK: same squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,
    
    // Config PDA derived from squad_multisig
    #[account(
        seeds = [b"config", squad_multisig.key().as_ref()],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,
    
    // zBTC mint
    pub zbtc_mint: Account<'info, Mint>,
    
    // sBTC mint
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,
    
    // User's zBTC account
    #[account(
        mut,
        constraint = user_zbtc_account.mint == zbtc_mint.key() @ ErrorCode::InvalidTokenMint
    )]
    pub user_zbtc_account: Account<'info, TokenAccount>,
    
    // User's sBTC account
    #[account(
        mut,
        constraint = user_sbtc_account.mint == sbtc_mint.key() @ ErrorCode::InvalidTokenMint
    )]
    pub user_sbtc_account: Account<'info, TokenAccount>,
    
    // Treasury vault PDA (derived from the same squad_multisig)
    #[account(
        mut,
        seeds = [b"treasury", squad_multisig.key().as_ref()],
        bump,
        token::mint = zbtc_mint
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,

    // Fee vault PDA (derived from the same squad_multisig)
    #[account(
        mut,
        seeds = [b"fees", squad_multisig.key().as_ref()],
        bump,
        token::mint = zbtc_mint
    )]
    pub fee_vault: Account<'info, TokenAccount>,
    
    // sBTC mint authority PDA
    /// CHECK: PDA for sBTC mint authority
    #[account(seeds = [b"sbtc_mint_authority"], bump)]
    pub sbtc_mint_authority_pda: UncheckedAccount<'info>,
    
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct BurnSbtc<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// the multisig used as the seed for config/treasury/fees (does not need to be a signer here)
    /// supply the same squad_multisig that was used during initialize
    /// CHECK: same squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,

    // Config PDA derived from squad_multisig
    #[account(
        seeds = [b"config", squad_multisig.key().as_ref()],
        bump = config.bump
    )]
    pub config: Account<'info, Config>,

    // zBTC mint
    pub zbtc_mint: Account<'info, Mint>,

    // sBTC mint
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,

    // User accounts
    #[account(mut)]
    pub user_zbtc_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user_sbtc_account: Account<'info, TokenAccount>,

    // Treasury vault PDA (derived from the same squad_multisig)
    #[account(
        mut,
        seeds = [b"treasury", squad_multisig.key().as_ref()],
        bump,
        token::mint = zbtc_mint
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,

    // Fee vault PDA (derived from the same squad_multisig)
    #[account(
        mut,
        seeds = [b"fees", squad_multisig.key().as_ref()],
        bump,
        token::mint = zbtc_mint
    )]
    pub fee_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub squad_multisig: Pubkey,
    pub sbtc_mint: Pubkey,
    pub zbtc_mint: Pubkey,
    pub treasury_zbtc_vault: Pubkey,
    pub fee_vault: Pubkey,
    pub sbtc_mint_authority_pda: Pubkey,
    pub fee_rate_bps: u64,
    pub min_collateral_bps: u64,
    pub bump: u8,
}

// ========================= Events ================================
#[event]
pub struct InitializedEvent {
    pub squad_multisig: Pubkey,
    pub sbtc_mint: Pubkey,
    pub zbtc_mint: Pubkey,
    pub treasury_vault: Pubkey,
    pub fee_vault: Pubkey,
    pub fee_rate_bps: u64,
    pub min_collateral_bps: u64,
    pub timestamp: i64,
}

#[event]
pub struct MintEvent {
    pub user: Pubkey,
    pub zbtc_amount: u64,
    pub sbtc_minted: u64,
    pub fee_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct BurnEvent {
    pub user: Pubkey,
    pub sbtc_burned: u64,
    pub zbtc_redeemed: u64,
    pub fee_amount: u64,
    pub timestamp: i64,
}
// ========================= Errors ================================
#[error_code]
pub enum ErrorCode {
    #[msg("Fee rate must be 5% or less")]
    InvalidFeeRate,
    #[msg("Collateral ratio must be at least 10%")]
    InvalidCollateralRatio,
    #[msg("sBTC mint must have Squad as initial authority")]
    InvalidMintAuthority,
    #[msg("sBTC mint must have Squad or no freeze authority")]
    InvalidFreezeAuthority,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Invalid zBTC mint")]
    InvalidZbtcMint,
    #[msg("Invalid sBTC mint")]
    InvalidSbtcMint,
    #[msg("Invalid token account owner")]
    InvalidTokenAccountOwner,
    #[msg("Invalid token mint")]
    InvalidTokenMint,
    #[msg("Insufficient balance")]
    InsufficientBalance,
}
