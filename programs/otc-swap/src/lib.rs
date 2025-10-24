// Stops Rust Analyzer complaining about missing configs
// See https://solana.stackexchange.com/questions/17777
#![allow(unexpected_cfgs)]

// Fix warning: use of deprecated method `anchor_lang::prelude::AccountInfo::<'a>::realloc`: Use AccountInfo::resize() instead
// See https://solana.stackexchange.com/questions/22979
#![allow(deprecated)]

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token::{self, SetAuthority, Mint, Token, TokenAccount, Transfer, MintTo, Burn};
use spl_token::instruction::AuthorityType;
use pyth_sdk_solana::{Price, PriceFeed, PythError};
use pyth_sdk_solana::state::SolanaPriceAccount;


const CONFIG_MAX_FEE_RATE_BPS: u64 = 500;
const CONFIG_MIN_COLLATERAL_BPS: u64 = 20_000;
const ORACLE_MAX_AGE: u64 = 300;


declare_id!("DBHmndyfN4j7BtQsLaCR1SPd7iAXaf1ezUicDs3pUXS8");

#[program]
pub mod otc_swap {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        fee_rate_bps: u64,
        min_collateral_bps: u64,
        authorized_zbtc_pyth_feed: Pubkey,
        authorized_sbtc_oracle_state_pda: Pubkey,
    ) -> Result<()> {
        require!(fee_rate_bps <= CONFIG_MAX_FEE_RATE_BPS, ErrorCode::InvalidFeeRate,);
        require!(min_collateral_bps >= CONFIG_MIN_COLLATERAL_BPS, ErrorCode::InvalidCollateralRatio,);
        require!(
            ctx.accounts.sbtc_mint.mint_authority == COption::Some(ctx.accounts.squad_multisig.key()),
            ErrorCode::InvalidMintAuthority,
        );

        require!(
            ctx.accounts.sbtc_mint.freeze_authority == COption::Some(ctx.accounts.squad_multisig.key()),
            ErrorCode::InvalidFreezeAuthority,
        );

        require!(
            ctx.accounts.treasury_zbtc_vault.mint == ctx.accounts.zbtc_mint.key(),
            ErrorCode::InvalidZbtcMint,
        );

        require!(
            ctx.accounts.fee_vault.mint == ctx.accounts.zbtc_mint.key(),
            ErrorCode::InvalidZbtcMint,
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
        
        let timestamp = Clock::get()?.unix_timestamp;
        let config = &mut ctx.accounts.config;
        config.squad_multisig = ctx.accounts.squad_multisig.key();
        config.sbtc_mint = ctx.accounts.sbtc_mint.key();
        config.zbtc_mint = ctx.accounts.zbtc_mint.key();
        config.treasury_zbtc_vault = ctx.accounts.treasury_zbtc_vault.key();
        config.fee_vault = ctx.accounts.fee_vault.key();
        config.fee_rate_bps = fee_rate_bps;
        config.min_collateral_bps = min_collateral_bps;
        config.bump = ctx.bumps.config;
        config.sbtc_decimals = ctx.accounts.sbtc_mint.decimals;
        config.zbtc_decimals = ctx.accounts.zbtc_mint.decimals;
        config.paused = false;
        config.total_sbtc_outstanding = 0u128;
        config.created_at = timestamp;
        config.authorized_zbtc_pyth_feed = authorized_zbtc_pyth_feed;
        config.authorized_sbtc_oracle_state_pda = authorized_sbtc_oracle_state_pda;

        emit!(InitializedEvent {
            squad_multisig: ctx.accounts.squad_multisig.key(),
            sbtc_mint: ctx.accounts.sbtc_mint.key(),
            zbtc_mint: ctx.accounts.zbtc_mint.key(),
            treasury_vault: ctx.accounts.treasury_zbtc_vault.key(),
            fee_vault: ctx.accounts.fee_vault.key(),
            fee_rate_bps,
            min_collateral_bps,
            timestamp: timestamp,
            sbtc_mint_authority: ctx.accounts.sbtc_mint_authority_pda.key(),
            treasury_vault_authority: ctx.accounts.treasury_authority_pda.key(),
            fee_vault_authority: ctx.accounts.fee_authority_pda.key(),
            authorized_zbtc_pyth_feed: authorized_zbtc_pyth_feed,
            authorized_sbtc_oracle_state_pda: authorized_sbtc_oracle_state_pda,
        });
        
        Ok(())
    }

    pub fn mint_sbtc(ctx: Context<MintSbtc>, zbtc_amount: u64) -> Result<()> {
        msg!("=== START MINT_SBTC ===");

        // -- 1) basic validation
        require!(zbtc_amount > 0, ErrorCode::InvalidAmount);
        let config: &mut Account<'_, Config> = &mut ctx.accounts.config;

        require!(!config.paused, ErrorCode::Paused);
        require!(ctx.accounts.zbtc_mint.key() == config.zbtc_mint, ErrorCode::InvalidZbtcMint);
        require!(ctx.accounts.sbtc_mint.key() == config.sbtc_mint, ErrorCode::InvalidSbtcMint);
        require!(ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_zbtc_account.amount >= zbtc_amount, ErrorCode::InsufficientBalance);
        msg!("DEBUG: Passed all account validations");

        // -- 2) compute fee and net deposit (u128 math)
        let fee_bps = config.fee_rate_bps as u128;
        let zbtc_amount_u128 = zbtc_amount as u128;
        let fee_amount_u128 = zbtc_amount_u128
            .checked_mul(fee_bps)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10_000u128) // using bps base
            .ok_or(ErrorCode::InvalidAmount)?;

        let fee_amount_u64 = fee_amount_u128 as u64; // safe because fee <= zbtc_amount which is u64
        let net_zbtc_u128 = zbtc_amount_u128.checked_sub(fee_amount_u128).ok_or(ErrorCode::InvalidAmount)?;
        let net_zbtc_u64 = net_zbtc_u128 as u64;
        msg!("DEBUG: Fee calculation complete");

        // -- 3) read & validatezBTC/USD price from Pyth feed
        let pyth_account = &ctx.accounts.pyth_price_account;

        let price_feed: PriceFeed = SolanaPriceAccount::account_info_to_feed(pyth_account)
            .map_err(|e: PythError| {
                msg!("Pyth error: {:?}", e);
                ErrorCode::PythError
            })?;
        msg!("DEBUG: Pyth feed loaded");

        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;
        let max_age = ORACLE_MAX_AGE;

        // mainnet only
        // let current_price_opt: Option<Price> = price_feed.get_price_no_older_than(current_time, max_age);
        let current_price_opt: Option<Price> = Some(price_feed.get_price_unchecked());
        let price: Price = current_price_opt.ok_or(ErrorCode::StaleOraclePrice)?;
        msg!("price: {:?}", price);

        require!(price.conf < price.price.unsigned_abs() / 1000u64, ErrorCode::HighConfidence);
        msg!("DEBUG: Pyth price confidence check passed");

        let zbtc_price_cents: u64 = if price.price >= 0 {
            // Pyth price format: actual_price = price * 10^expo
            // price_cents = actual_price * 100 = price * 10^expo * 100 = price * 10^(expo + 2)
            let actual_expo = price.expo + 2; // +2 to convert to cents
            
            if actual_expo >= 0 {
                (price.price as u64).checked_mul(10u64.pow(actual_expo as u32))
                    .ok_or(ErrorCode::InvalidPrice)?
            } else {
                (price.price as u64).checked_div(10u64.pow((-actual_expo) as u32))
                    .ok_or(ErrorCode::InvalidPrice)?
            }
        } else {
            return Err(anchor_lang::error::Error::from(ErrorCode::InvalidPrice));
        };
        msg!("price in cents: {:?}", zbtc_price_cents);
        msg!("DEBUG: Pyth price conversion complete");

        // -- 4) Get sBTC price from oracle
        let oracle_account_data = ctx.accounts.oracle_state.try_borrow_data()?;
        msg!("DEBUG: Oracle account data length: {}", oracle_account_data.len());

        // CRITICAL: Check account has enough data before slicing
        require!(oracle_account_data.len() >= 24, ErrorCode::InvalidOracleData); // 8 discriminator + 8 trend_value + 8 last_update

        let _discriminator = &oracle_account_data[0..8];
        msg!("DEBUG: Discriminator read successfully");

        let oracle_data = &oracle_account_data[8..]; // Skip discriminator
        let sbtc_price_cents = u64::from_le_bytes(oracle_data[0..8].try_into().unwrap());
        msg!("DEBUG: trend_value bytes read successfully");
        let last_update = i64::from_le_bytes(oracle_data[8..16].try_into().unwrap());

        msg!("DEBUG: Read sbtc_price_cents: {}", sbtc_price_cents);
        msg!("DEBUG: Read last_update: {}", last_update);

        // mainnet only - check if oracle data is recent enough
        let current_timestamp = clock.unix_timestamp;
        // require!(current_timestamp - last_update <= 300, ErrorCode::StaleOraclePrice);

        // -- 5) Calculate sBTC to mint        
        let zbtc_decimals = config.zbtc_decimals;
        let sbtc_decimals = config.sbtc_decimals;

        msg!("DEBUG: zbtc_price_cents: {}", zbtc_price_cents);
        msg!("DEBUG: sbtc_price_cents: {}", sbtc_price_cents);
        msg!("DEBUG: net_zbtc_u128: {}", net_zbtc_u128);
        msg!("DEBUG: zbtc_decimals: {}", zbtc_decimals);
        msg!("DEBUG: sbtc_decimals: {}", sbtc_decimals);
        
        // Convert net zBTC amount to USD value (in cents)
        let net_zbtc_value_cents = net_zbtc_u128
            .checked_mul(zbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        let sbtc_to_mint_u128 = net_zbtc_u128
            .checked_mul(zbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_mul(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        require!(sbtc_to_mint_u128 > 0, ErrorCode::InvalidAmount);
        require!(sbtc_to_mint_u128 <= u64::MAX as u128, ErrorCode::InvalidAmount);
        let sbtc_to_mint_u64 = sbtc_to_mint_u128 as u64;

        msg!("DEBUG: net_zbtc_value_cents: {}", net_zbtc_value_cents);
        msg!("DEBUG: sbtc_to_mint_u128: {}", sbtc_to_mint_u128);
        msg!("DEBUG: sbtc_to_mint_u64: {}", sbtc_to_mint_u64);

        // -- 6) Transfer zBTC to treasury and fee vault
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_zbtc_account.to_account_info(),
                    to: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            net_zbtc_u64,
        )?;

        if fee_amount_u64 > 0 {
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.user_zbtc_account.to_account_info(),
                        to: ctx.accounts.fee_vault.to_account_info(),
                        authority: ctx.accounts.user.to_account_info(),
                    },
                ),
                fee_amount_u64,
            )?;
        }

        // -- 7) Mint sBTC to user
        let seeds = &[
            b"sbtc_mint_authority",
            ctx.accounts.squad_multisig.key.as_ref(),
            &[ctx.bumps.sbtc_mint_authority_pda],
        ];
        let signer_seeds = &[&seeds[..]];

        let cpi_accounts = MintTo {
            mint: ctx.accounts.sbtc_mint.to_account_info(),
            to: ctx.accounts.user_sbtc_account.to_account_info(),
            authority: ctx.accounts.sbtc_mint_authority_pda.to_account_info(),
        };

        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::mint_to(
            CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds),
            sbtc_to_mint_u64,
        )?;

        // -- 8) Update accounting
        config.total_sbtc_outstanding = config.total_sbtc_outstanding
            .checked_add(sbtc_to_mint_u64 as u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        // -- 9) Collateral check
        let treasury_balance = ctx.accounts.treasury_zbtc_vault.amount as u128;
        
        // Calculate required collateral: total_sbtc_outstanding * sbtc_price / zbtc_price
        let required_zbtc_minor = config.total_sbtc_outstanding
            .checked_mul(sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_mul(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(zbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        // Apply collateral buffer
        let min_collateral_bps = config.min_collateral_bps as u128;
        let required_zbtc_with_buffer = required_zbtc_minor
            .checked_mul(min_collateral_bps)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10_000u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        require!(treasury_balance >= required_zbtc_with_buffer, ErrorCode::InsufficientCollateral);

        // -- 10) Emit event
        emit!(MintEvent {
            user: ctx.accounts.user.key(),
            zbtc_deposited: zbtc_amount,
            sbtc_minted: sbtc_to_mint_u64 as u128,
            fee_amount: fee_amount_u64,
            zbtc_price_cents,
            sbtc_price_cents,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn burn_sbtc(ctx: Context<BurnSbtc>, sbtc_amount: u64) -> Result<()> {
        require!(sbtc_amount > 0, ErrorCode::InvalidAmount);

        let config = &mut ctx.accounts.config;
        require!(!config.paused, ErrorCode::Paused);
        require!(ctx.accounts.zbtc_mint.key() == config.zbtc_mint, ErrorCode::InvalidZbtcMint);
        require!(ctx.accounts.sbtc_mint.key() == config.sbtc_mint, ErrorCode::InvalidSbtcMint);
        require!(ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_sbtc_account.amount >= sbtc_amount, ErrorCode::InsufficientBalance);

        // -- 1) Get zBTC/USD price from Pyth
        let pyth_account = &ctx.accounts.pyth_price_account;
        let price_feed: PriceFeed = SolanaPriceAccount::account_info_to_feed(pyth_account)
            .map_err(|e: PythError| {
                msg!("Pyth error: {:?}", e);
                ErrorCode::PythError
            })?;

        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;
        let max_age = ORACLE_MAX_AGE;

        // mainnet only
        // let current_price_opt: Option<Price> = price_feed.get_price_no_older_than(current_time, max_age);
        let current_price_opt: Option<Price> = Some(price_feed.get_price_unchecked());
        let price: Price = current_price_opt.ok_or(ErrorCode::StaleOraclePrice)?;
        msg!("price: {:?}", price);

        require!(price.conf < price.price.unsigned_abs() / 1000u64, ErrorCode::HighConfidence);

        // Convert Pyth price to cents (USD)
        let zbtc_price_cents: u64 = if price.price >= 0 {
            let actual_expo = price.expo + 2;
            
            if actual_expo >= 0 {
                (price.price as u64).checked_mul(10u64.pow(actual_expo as u32))
                    .ok_or(ErrorCode::InvalidPrice)?
            } else {
                (price.price as u64).checked_div(10u64.pow((-actual_expo) as u32))
                    .ok_or(ErrorCode::InvalidPrice)?
            }
        } else {
            return Err(anchor_lang::error::Error::from(ErrorCode::InvalidPrice));
        };

        // -- 2) Get sBTC price from your oracle
        let oracle_account_data = ctx.accounts.oracle_state.try_borrow_data()?;
        let oracle_data = &oracle_account_data[8..];
        
        let sbtc_price_cents = u64::from_le_bytes(oracle_data[0..8].try_into().unwrap());
        let last_update = i64::from_le_bytes(oracle_data[8..16].try_into().unwrap());

        // mainnet only
        // require!(current_time - last_update <= 300, ErrorCode::StaleOraclePrice);

        // -- 3) Calculate zBTC to redeem
        let zbtc_decimals = config.zbtc_decimals;
        let sbtc_decimals = config.sbtc_decimals;
        
        // zbtc_to_redeem = (sbtc_amount * sbtc_price_cents * 10^zbtc_decimals) / (zbtc_price_cents * 10^sbtc_decimals)
        let zbtc_to_redeem_u128 = (sbtc_amount as u128)
            .checked_mul(sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_mul(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(zbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        require!(zbtc_to_redeem_u128 > 0, ErrorCode::InvalidAmount);
        require!(zbtc_to_redeem_u128 <= u64::MAX as u128, ErrorCode::InvalidAmount);
        let zbtc_to_redeem_u64 = zbtc_to_redeem_u128 as u64;

        // -- 4) Calculate fee and net redemption
        let fee_bps = config.fee_rate_bps as u128;
        let fee_amount_u128 = zbtc_to_redeem_u128
            .checked_mul(fee_bps)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10_000u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        let fee_amount_u64 = fee_amount_u128 as u64;
        let net_zbtc_u128 = zbtc_to_redeem_u128.checked_sub(fee_amount_u128).ok_or(ErrorCode::InvalidAmount)?;
        let net_zbtc_u64 = net_zbtc_u128 as u64;

        require!(net_zbtc_u64 > 0, ErrorCode::InvalidAmount);

        // -- 5) Treasury liquidity check
        require!(ctx.accounts.treasury_zbtc_vault.amount >= zbtc_to_redeem_u64, ErrorCode::InsufficientLiquidity);

        // -- 6) Burn sBTC
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

        // -- 7) Transfer net redemption to user
        let seeds: &[&[u8]] = &[
            b"treasury_auth_v1",
            ctx.accounts.squad_multisig.key.as_ref(),
            &[ctx.bumps.treasury_authority_pda],
        ];
        let signer_seeds = &[seeds];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                    to: ctx.accounts.user_zbtc_account.to_account_info(),
                    authority: ctx.accounts.treasury_authority_pda.to_account_info(),
                },
                signer_seeds,
            ),
            net_zbtc_u64,
        )?;

        // -- 8) Transfer fee to fee vault
        if fee_amount_u64 > 0 {
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.treasury_zbtc_vault.to_account_info(),
                        to: ctx.accounts.fee_vault.to_account_info(),
                        authority: ctx.accounts.treasury_authority_pda.to_account_info(),
                    },
                    signer_seeds,
                ),
                fee_amount_u64,
            )?;
        }

        // -- 9) Update accounting
        config.total_sbtc_outstanding = config.total_sbtc_outstanding
            .checked_sub(sbtc_amount as u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        // -- 10) Collateral check after burn
        let treasury_balance = ctx.accounts.treasury_zbtc_vault.amount as u128;
        
        let required_zbtc_minor = config.total_sbtc_outstanding
            .checked_mul(sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_mul(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(zbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        let min_collateral_bps = config.min_collateral_bps as u128;
        let required_zbtc_with_buffer = required_zbtc_minor
            .checked_mul(min_collateral_bps)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10_000u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        require!(treasury_balance >= required_zbtc_with_buffer, ErrorCode::InsufficientCollateral);

        // -- 11) Emit event
        emit!(BurnEvent {
            user: ctx.accounts.user.key(),
            sbtc_burned: sbtc_amount,
            zbtc_redeemed: net_zbtc_u64,
            fee_amount: fee_amount_u64,
            zbtc_price_cents,
            sbtc_price_cents,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

}

// ========================= Accounts / PDAs ================================
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub squad_multisig: Signer<'info>,

    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,

    pub zbtc_mint: Account<'info, Mint>,
    
    /// CHECK: PDA that will become sBTC mint authority
    #[account(seeds = [b"sbtc_mint_authority", squad_multisig.key().as_ref()], bump)]
    pub sbtc_mint_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for treasury token account
    #[account(seeds = [b"treasury_auth_v1", squad_multisig.key().as_ref()], bump)]
    pub treasury_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for fee token account
    #[account(seeds = [b"fee_auth_v1", squad_multisig.key().as_ref()], bump)]
    pub fee_authority_pda: UncheckedAccount<'info>,

    #[account(
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,

    #[account(
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
    )]
    pub fee_vault: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = squad_multisig,
        space = 8 + Config::INIT_SPACE,
        seeds = [b"config_v1", squad_multisig.key().as_ref()],
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

    /// CHECK: must match config.squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"config_v1", squad_multisig.key().as_ref()],
        bump = config.bump,
        constraint = config.squad_multisig == squad_multisig.key() @ ErrorCode::InvalidSquadMultisig,
    )]
    pub config: Box<Account<'info, Config>>,

    pub zbtc_mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub sbtc_mint: Box<Account<'info, Mint>>,

    #[account(
        mut, 
        constraint = user_zbtc_account.mint == zbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_zbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_zbtc_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut, 
        constraint = user_sbtc_account.mint == sbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_sbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_sbtc_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
        constraint = treasury_zbtc_vault.key() == config.treasury_zbtc_vault @ ErrorCode::InvalidTreasuryVault,
    )]
    pub treasury_zbtc_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
        constraint = fee_vault.key() == config.fee_vault @ ErrorCode::InvalidFeeVault,
    )]
    pub fee_vault: Box<Account<'info, TokenAccount>>,

    /// CHECK: PDA for sBTC mint authority
    #[account(
        seeds = [b"sbtc_mint_authority", squad_multisig.key().as_ref()], 
        bump,
    )]
    pub sbtc_mint_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for treasury token account
    #[account(
        seeds = [b"treasury_auth_v1", squad_multisig.key().as_ref()], 
        bump,
    )]
    pub treasury_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for fee token account
    #[account(
        seeds = [b"fee_auth_v1", squad_multisig.key().as_ref()], 
        bump,
    )]
    pub fee_authority_pda: UncheckedAccount<'info>,

    /// CHECK: price account for the oracle (Pyth-style)
    pub pyth_price_account: UncheckedAccount<'info>,

    /// CHECK: We're manually deserializing this account
    pub oracle_state: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct BurnSbtc<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: same squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"config_v1", squad_multisig.key().as_ref()],
        bump = config.bump,
        constraint = config.squad_multisig == squad_multisig.key() @ ErrorCode::InvalidSquadMultisig,
    )]
    pub config: Box<Account<'info, Config>>,

    pub zbtc_mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub sbtc_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        constraint = user_zbtc_account.mint == zbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_zbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_zbtc_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = user_sbtc_account.mint == sbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_sbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_sbtc_account: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
        constraint = treasury_zbtc_vault.key() == config.treasury_zbtc_vault @ ErrorCode::InvalidTreasuryVault,
    )]
    pub treasury_zbtc_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
        constraint = fee_vault.key() == config.fee_vault @ ErrorCode::InvalidFeeVault,
    )]
    pub fee_vault: Box<Account<'info, TokenAccount>>,

    /// CHECK: PDA used as authority for treasury token account
    #[account(
        seeds = [b"treasury_auth_v1", squad_multisig.key().as_ref()], 
        bump,
    )]
    pub treasury_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for fee token account  
    #[account(
        seeds = [b"fee_auth_v1", squad_multisig.key().as_ref()],
        bump,
    )]
    pub fee_authority_pda: UncheckedAccount<'info>,

    /// CHECK: price account for the oracle (Pyth-style)
    pub pyth_price_account: UncheckedAccount<'info>,

    /// CHECK: We're manually deserializing this account
    pub oracle_state: AccountInfo<'info>,

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
    pub fee_rate_bps: u64,
    pub min_collateral_bps: u64,
    pub bump: u8,
    pub sbtc_decimals: u8,
    pub zbtc_decimals: u8,
    pub paused: bool,
    pub total_sbtc_outstanding: u128,
    pub created_at: i64,
    pub authorized_zbtc_pyth_feed: Pubkey,
    pub authorized_sbtc_oracle_state_pda: Pubkey,
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
    pub sbtc_mint_authority: Pubkey,
    pub treasury_vault_authority: Pubkey,
    pub fee_vault_authority: Pubkey,
    pub authorized_zbtc_pyth_feed: Pubkey,
    pub authorized_sbtc_oracle_state_pda: Pubkey,
}

#[event]
pub struct MintEvent {
    pub user: Pubkey,
    pub zbtc_deposited: u64,
    pub sbtc_minted: u128,
    pub fee_amount: u64,
    pub zbtc_price_cents: u64,
    pub sbtc_price_cents: u64,
    pub timestamp: i64,
}

#[event]
pub struct BurnEvent {
    pub user: Pubkey,
    pub sbtc_burned: u64,
    pub zbtc_redeemed: u64,
    pub fee_amount: u64,
    pub zbtc_price_cents: u64,
    pub sbtc_price_cents: u64,
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
    #[msg("Protocol paused")]
    Paused,
    #[msg("Insufficient liquidity")]
    InsufficientLiquidity,
    #[msg("Insufficient collateral")]
    InsufficientCollateral,
    #[msg("Invalid squad multisig")]
    InvalidSquadMultisig,
    #[msg("Invalid treasury vault")]
    InvalidTreasuryVault,
    #[msg("Invalid fee vault")]
    InvalidFeeVault,
    #[msg("Invalid token owner")]
    InvalidTokenOwner,
    #[msg("Pyth oracle error")]
    PythError,
    #[msg("Invalid oracle data")]
    InvalidOracleData,
    #[msg("Stale price data")]
    StaleOraclePrice,
    #[msg("Invalid price value")]
    InvalidPrice,
    #[msg("High confidence interval - unreliable data")]
    HighConfidence,
}
