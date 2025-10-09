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


declare_id!("DBHmndyfN4j7BtQsLaCR1SPd7iAXaf1ezUicDs3pUXS8");

#[program]
pub mod otc_swap {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, fee_rate_bps: u64, min_collateral_bps: u64) -> Result<()> {
        // VALIDATION
        require!(fee_rate_bps <= 500, ErrorCode::InvalidFeeRate,); // Max 5%
        require!(min_collateral_bps >= 20_000, ErrorCode::InvalidCollateralRatio,); // 100 BPS == 1% --> 20_000 BPS == 200%
        require!(
            ctx.accounts.sbtc_mint.mint_authority == COption::Some(ctx.accounts.squad_multisig.key()),
            ErrorCode::InvalidMintAuthority,
        );

        // Check freeze authority
        require!(
            ctx.accounts.sbtc_mint.freeze_authority == COption::Some(ctx.accounts.squad_multisig.key()),
            ErrorCode::InvalidFreezeAuthority,
        );

        // Check that the token accounts are for the expected mint (zBTC), and their owner is the token program or expected PDA
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
        
        // STORE CONFIG
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
        config.created_at = Clock::get()?.unix_timestamp;

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
            sbtc_mint_authority: ctx.accounts.sbtc_mint_authority_pda.key(),
            treasury_vault_authority: ctx.accounts.treasury_authority_pda.key(),
            fee_vault_authority: ctx.accounts.fee_authority_pda.key(),
        });
        
        Ok(())
    }

    pub fn mint_sbtc(ctx: Context<MintSbtc>, zbtc_amount: u64) -> Result<()> {
        // -- 1) basic validation
        require!(zbtc_amount > 0, ErrorCode::InvalidAmount); // maybe put a lower limit on sBTC when mint/burn to avoid dust
        // like this:
        // require!(sbtc_amount >= 10u64.pow(config.sbtc_decimals as u32) / 1000, ErrorCode::InvalidAmount); 

        let config: &mut Account<'_, Config> = &mut ctx.accounts.config;

        require!(!config.paused, ErrorCode::Paused);
        require!(ctx.accounts.zbtc_mint.key() == config.zbtc_mint, ErrorCode::InvalidZbtcMint);
        require!(ctx.accounts.sbtc_mint.key() == config.sbtc_mint, ErrorCode::InvalidSbtcMint);
        require!(ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_zbtc_account.amount >= zbtc_amount, ErrorCode::InsufficientBalance);

        // -- 2) compute fee and net deposit (u128 math)
        let fee_bps = config.fee_rate_bps as u128; // bps: e.g. 500 means 5.00%
        let zbtc_amount_u128 = zbtc_amount as u128;
        let fee_amount_u128 = zbtc_amount_u128
            .checked_mul(fee_bps)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10_000u128) // using 10_000 bps base
            .ok_or(ErrorCode::InvalidAmount)?;

        let fee_amount_u64 = fee_amount_u128 as u64; // safe because fee <= zbtc_amount which is u64
        let net_zbtc_u128 = zbtc_amount_u128.checked_sub(fee_amount_u128).ok_or(ErrorCode::InvalidAmount)?;
        let net_zbtc_u64 = net_zbtc_u128 as u64;

        // -- 3) read & validatezBTC/USD price from Pyth feed
        let pyth_account = &ctx.accounts.pyth_price_account;

        let price_feed: PriceFeed = SolanaPriceAccount::account_info_to_feed(pyth_account)
            .map_err(|e: PythError| {
                msg!("Pyth error: {:?}", e);
                ErrorCode::PythError
            })?;

        let clock = Clock::get()?;
        let current_time = clock.unix_timestamp;
        let max_age = 60u64;

        let current_price_opt: Option<Price> = price_feed.get_price_no_older_than(current_time, max_age);

        let price: Price = current_price_opt.ok_or(ErrorCode::StaleOraclePrice)?;

        // Confidence check - ensure confidence is reasonable
        require!(price.conf < price.price.unsigned_abs() / 1000u64, ErrorCode::HighConfidence);

        // Convert Pyth price to cents (USD)
        let zbtc_price_cents: u64 = if price.price >= 0 {
            // Convert from Pyth's base units to cents
            // Pyth price: price * 10^expo, so to get cents: (price * 10^expo) / (10^(-2)) = price * 10^(expo + 2)
            let price_in_cents = (price.price as u64) * 10u64.pow((price.expo + 2) as u32);
            price_in_cents
        } else {
            return Err(anchor_lang::error::Error::from(ErrorCode::InvalidPrice));
        };

        // -- 4) Get sBTC price from your oracle
        let oracle_account_data = ctx.accounts.oracle_state.try_borrow_data()?;
        let oracle_data = &oracle_account_data[8..]; // Skip discriminator
        
        let sbtc_price_cents = u64::from_le_bytes(oracle_data[0..8].try_into().unwrap());
        let last_update = i64::from_le_bytes(oracle_data[8..16].try_into().unwrap());
        
        // Check if oracle data is recent enough (e.g., within 5 minutes)
        let current_timestamp = clock.unix_timestamp;
        require!(current_timestamp - last_update <= 300, ErrorCode::StaleOraclePrice);

        // -- 5) Calculate sBTC to mint
        // net_zbtc_value_usd = net_zbtc_amount * (zbtc_price_cents / 10^zbtc_decimals)
        // sbtc_to_mint = net_zbtc_value_usd / (sbtc_price_cents / 10^sbtc_decimals)
        
        let zbtc_decimals = config.zbtc_decimals;
        let sbtc_decimals = config.sbtc_decimals;
        
        // Convert net zBTC amount to USD value (in cents)
        let net_zbtc_value_cents = net_zbtc_u128
            .checked_mul(zbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        // Calculate sBTC to mint: (net_zbtc_value_cents * 10^sbtc_decimals) / sbtc_price_cents
        let sbtc_to_mint_u128 = net_zbtc_value_cents
            .checked_mul(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        require!(sbtc_to_mint_u128 > 0, ErrorCode::InvalidAmount);
        require!(sbtc_to_mint_u128 <= u64::MAX as u128, ErrorCode::InvalidAmount);
        let sbtc_to_mint_u64 = sbtc_to_mint_u128 as u64;

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

        // -- 9) Collateral check (simplified version)
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

    pub fn mint_sbtc_test(
        ctx: Context<MintSbtcTest>, 
        zbtc_amount: u64,
        mock_zbtc_price_cents: u64,
        mock_sbtc_price_cents: u64
    ) -> Result<()> {
        // -- 1) basic validation
        require!(zbtc_amount > 0, ErrorCode::InvalidAmount);
        
        let config: &mut Account<'_, Config> = &mut ctx.accounts.config;

        require!(!config.paused, ErrorCode::Paused);
        require!(ctx.accounts.zbtc_mint.key() == config.zbtc_mint, ErrorCode::InvalidZbtcMint);
        require!(ctx.accounts.sbtc_mint.key() == config.sbtc_mint, ErrorCode::InvalidSbtcMint);
        require!(ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_zbtc_account.amount >= zbtc_amount, ErrorCode::InsufficientBalance);

        // -- 2) compute fee and net deposit (u128 math)
        let fee_bps = config.fee_rate_bps as u128; // bps: e.g. 500 means 5.00%
        let zbtc_amount_u128 = zbtc_amount as u128;
        let fee_amount_u128 = zbtc_amount_u128
            .checked_mul(fee_bps)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10_000u128) // using 10_000 bps base
            .ok_or(ErrorCode::InvalidAmount)?;

        let fee_amount_u64 = fee_amount_u128 as u64; // safe because fee <= zbtc_amount which is u64
        let net_zbtc_u128 = zbtc_amount_u128.checked_sub(fee_amount_u128).ok_or(ErrorCode::InvalidAmount)?;
        let net_zbtc_u64 = net_zbtc_u128 as u64;

        // -- 3-4) use mock prices instead of Pyth/oracle calls
        let zbtc_price_cents = mock_zbtc_price_cents;
        let sbtc_price_cents = mock_sbtc_price_cents;

        // -- 5) Calculate sBTC to mint
        // net_zbtc_value_usd = net_zbtc_amount * (zbtc_price_cents / 10^zbtc_decimals)
        // sbtc_to_mint = net_zbtc_value_usd / (sbtc_price_cents / 10^sbtc_decimals)
        let zbtc_decimals = config.zbtc_decimals;
        let sbtc_decimals = config.sbtc_decimals;
        
        // Convert net zBTC amount to USD value (in cents)
        let net_zbtc_value_cents = net_zbtc_u128
            .checked_mul(zbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        // Calculate sBTC to mint: (net_zbtc_value_cents * 10^sbtc_decimals) / sbtc_price_cents
        let sbtc_to_mint_u128 = net_zbtc_value_cents
            .checked_mul(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        require!(sbtc_to_mint_u128 > 0, ErrorCode::InvalidAmount);
        require!(sbtc_to_mint_u128 <= u64::MAX as u128, ErrorCode::InvalidAmount);
        let sbtc_to_mint_u64 = sbtc_to_mint_u128 as u64;

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

        // -- 9) Collateral check (simplified version)
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
        require!(sbtc_amount > 0, ErrorCode::InvalidAmount); // maybe put a lower limit on sBTC when mint/burn to avoid dust
        // like this:
        // require!(sbtc_amount >= 10u64.pow(config.sbtc_decimals as u32) / 1000, ErrorCode::InvalidAmount);

        let config = &mut ctx.accounts.config;

        // Paused check
        require!(!config.paused, ErrorCode::Paused);

        // Mint sanity
        require!(ctx.accounts.zbtc_mint.key() == config.zbtc_mint, ErrorCode::InvalidZbtcMint);
        require!(ctx.accounts.sbtc_mint.key() == config.sbtc_mint, ErrorCode::InvalidSbtcMint);

        // User owns token accounts
        require!(ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);

        // Balance check
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
        let max_age = 60u64;

        let current_price_opt: Option<Price> = price_feed.get_price_no_older_than(current_time, max_age);
        let price: Price = current_price_opt.ok_or(ErrorCode::StaleOraclePrice)?;

        // Confidence check
        require!(price.conf < price.price.unsigned_abs() / 1000u64, ErrorCode::HighConfidence);

        // Convert Pyth price to cents (USD)
        let zbtc_price_cents: u64 = if price.price >= 0 {
            (price.price as u64) * 10u64.pow((price.expo + 2) as u32)
        } else {
            return Err(anchor_lang::error::Error::from(ErrorCode::InvalidPrice));
        };

        // -- 2) Get sBTC price from your oracle
        let oracle_account_data = ctx.accounts.oracle_state.try_borrow_data()?;
        let oracle_data = &oracle_account_data[8..]; // Skip discriminator
        
        let sbtc_price_cents = u64::from_le_bytes(oracle_data[0..8].try_into().unwrap());
        let last_update = i64::from_le_bytes(oracle_data[8..16].try_into().unwrap());

        // Check if oracle data is recent enough
        require!(current_time - last_update <= 300, ErrorCode::StaleOraclePrice);

        // -- 3) Calculate zBTC to redeem
        // sbtc_value_usd = sbtc_amount * (sbtc_price_cents / 10^sbtc_decimals)
        // zbtc_to_redeem = sbtc_value_usd / (zbtc_price_cents / 10^zbtc_decimals)
        
        let zbtc_decimals = config.zbtc_decimals;
        let sbtc_decimals = config.sbtc_decimals;
        
        // Convert sBTC amount to USD value (in cents)
        let sbtc_value_cents = (sbtc_amount as u128)
            .checked_mul(sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        // Calculate zBTC to redeem: (sbtc_value_cents * 10^zbtc_decimals) / zbtc_price_cents
        let zbtc_to_redeem_u128 = sbtc_value_cents
            .checked_mul(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(zbtc_price_cents as u128)
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

        // -- 10) Emit event
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

    pub fn burn_sbtc_test(
        ctx: Context<BurnSbtcTest>,
        sbtc_amount: u64,
        mock_zbtc_price_cents: u64,
        mock_sbtc_price_cents: u64
    ) -> Result<()> {
        require!(sbtc_amount > 0, ErrorCode::InvalidAmount); // maybe put a lower limit on sBTC when mint/burn to avoid dust
        // like this:
        // require!(sbtc_amount >= 10u64.pow(config.sbtc_decimals as u32) / 1000, ErrorCode::InvalidAmount);

        let config = &mut ctx.accounts.config;

        // Paused check
        require!(!config.paused, ErrorCode::Paused);

        // Mint sanity
        require!(ctx.accounts.zbtc_mint.key() == config.zbtc_mint, ErrorCode::InvalidZbtcMint);
        require!(ctx.accounts.sbtc_mint.key() == config.sbtc_mint, ErrorCode::InvalidSbtcMint);

        // User owns token accounts
        require!(ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);

        // Balance check
        require!(ctx.accounts.user_sbtc_account.amount >= sbtc_amount, ErrorCode::InsufficientBalance);

        // -- 3) Calculate zBTC to redeem
        // sbtc_value_usd = sbtc_amount * (sbtc_price_cents / 10^sbtc_decimals)
        // zbtc_to_redeem = sbtc_value_usd / (zbtc_price_cents / 10^zbtc_decimals)
        
        let zbtc_decimals = config.zbtc_decimals;
        let sbtc_decimals = config.sbtc_decimals;
        
        // Convert sBTC amount to USD value (in cents)
        let sbtc_value_cents = (sbtc_amount as u128)
            .checked_mul(mock_sbtc_price_cents as u128)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10u128.pow(sbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?;

        // Calculate zBTC to redeem: (sbtc_value_cents * 10^zbtc_decimals) / zbtc_price_cents
        let zbtc_to_redeem_u128 = sbtc_value_cents
            .checked_mul(10u128.pow(zbtc_decimals as u32))
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(mock_zbtc_price_cents as u128)
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

        // -- 10) Emit event
        emit!(BurnEvent {
            user: ctx.accounts.user.key(),
            sbtc_burned: sbtc_amount,
            zbtc_redeemed: net_zbtc_u64,
            fee_amount: fee_amount_u64,
            zbtc_price_cents: mock_zbtc_price_cents,
            sbtc_price_cents: mock_sbtc_price_cents,
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
    
    // sBTC mint (pre-created, authority will be transferred)
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,
    
    // zBTC mint (already exists)
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

    // Treasury vault - token account whose authority is treasury_authority_pda
    #[account(
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,
    
    // Fee vault - token account whose authority is fee_authority_pda
    #[account(
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
    )]
    pub fee_vault: Account<'info, TokenAccount>,
    
    // Config PDA
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

    /// The same squad_multisig used at `initialize` (NOT a signer here)
    /// CHECK: must match config.squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,

    // Config PDA derived from squad_multisig
    #[account(
        mut,
        seeds = [b"config_v1", squad_multisig.key().as_ref()],
        bump = config.bump, // Only store this one bump
        constraint = config.squad_multisig == squad_multisig.key() @ ErrorCode::InvalidSquadMultisig,
    )]
    pub config: Account<'info, Config>,

    // zBTC mint
    pub zbtc_mint: Account<'info, Mint>,

    // sBTC mint
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,

    // User token accounts
    #[account(
        mut, 
        constraint = user_zbtc_account.mint == zbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_zbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_zbtc_account: Account<'info, TokenAccount>,
    
    #[account(
        mut, 
        constraint = user_sbtc_account.mint == sbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_sbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_sbtc_account: Account<'info, TokenAccount>,
    
    // Treasury vault
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
        constraint = treasury_zbtc_vault.key() == config.treasury_zbtc_vault @ ErrorCode::InvalidTreasuryVault,
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,

    // Fee vault
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
        constraint = fee_vault.key() == config.fee_vault @ ErrorCode::InvalidFeeVault,
    )]
    pub fee_vault: Account<'info, TokenAccount>,
    
    /// CHECK: PDA for sBTC mint authority
    #[account(
        seeds = [b"sbtc_mint_authority", squad_multisig.key().as_ref()], 
        bump, // Anchor will find the bump automatically
    )]
    pub sbtc_mint_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for treasury token account
    #[account(
        seeds = [b"treasury_auth_v1", squad_multisig.key().as_ref()], 
        bump, // Anchor will find the bump automatically
    )]
    pub treasury_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for fee token account
    #[account(
        seeds = [b"fee_auth_v1", squad_multisig.key().as_ref()], 
        bump, // Anchor will find the bump automatically
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

    /// The multisig used as the seed for config/treasury/fees (does not need to be a signer here)
    /// Supply the same squad_multisig that was used during initialize
    /// CHECK: same squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,

    // Config PDA derived from squad_multisig
    #[account(
        mut, // ← Need mut to update total_sbtc_outstanding
        seeds = [b"config_v1", squad_multisig.key().as_ref()],
        bump = config.bump,
        constraint = config.squad_multisig == squad_multisig.key() @ ErrorCode::InvalidSquadMultisig,
    )]
    pub config: Account<'info, Config>,

    // zBTC mint
    pub zbtc_mint: Account<'info, Mint>,

    // sBTC mint
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,

    // User accounts
    #[account(
        mut,
        constraint = user_zbtc_account.mint == zbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_zbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_zbtc_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = user_sbtc_account.mint == sbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_sbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_sbtc_account: Account<'info, TokenAccount>,

    // Treasury vault - NOT a PDA, regular token account
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
        constraint = treasury_zbtc_vault.key() == config.treasury_zbtc_vault @ ErrorCode::InvalidTreasuryVault,
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,

    // Fee vault - NOT a PDA, regular token account  
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
        constraint = fee_vault.key() == config.fee_vault @ ErrorCode::InvalidFeeVault,
    )]
    pub fee_vault: Account<'info, TokenAccount>,

    /// CHECK: PDA used as authority for treasury token account
    #[account(
        seeds = [b"treasury_auth_v1", squad_multisig.key().as_ref()], 
        bump, // Anchor finds bump automatically
    )]
    pub treasury_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for fee token account  
    #[account(
        seeds = [b"fee_auth_v1", squad_multisig.key().as_ref()],
        bump, // Anchor finds bump automatically
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
    pub total_sbtc_outstanding: u128, // track outstanding sBTC minted
    pub created_at: i64,
}

#[account]
pub struct OracleState {
    pub trend_value: u64,
    pub last_update: i64,
}

#[derive(Accounts)]
pub struct MintSbtcTest<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// The same squad_multisig used at `initialize` (NOT a signer here)
    /// CHECK: must match config.squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,

    // Config PDA derived from squad_multisig
    #[account(
        mut,
        seeds = [b"config_v1", squad_multisig.key().as_ref()],
        bump = config.bump, // Only store this one bump
        constraint = config.squad_multisig == squad_multisig.key() @ ErrorCode::InvalidSquadMultisig,
    )]
    pub config: Account<'info, Config>,

    // zBTC mint
    pub zbtc_mint: Account<'info, Mint>,

    // sBTC mint
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,

    // User token accounts
    #[account(
        mut, 
        constraint = user_zbtc_account.mint == zbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_zbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_zbtc_account: Account<'info, TokenAccount>,
    
    #[account(
        mut, 
        constraint = user_sbtc_account.mint == sbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_sbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_sbtc_account: Account<'info, TokenAccount>,
    
    // Treasury vault
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
        constraint = treasury_zbtc_vault.key() == config.treasury_zbtc_vault @ ErrorCode::InvalidTreasuryVault,
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,

    // Fee vault
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
        constraint = fee_vault.key() == config.fee_vault @ ErrorCode::InvalidFeeVault,
    )]
    pub fee_vault: Account<'info, TokenAccount>,
    
    /// CHECK: PDA for sBTC mint authority
    #[account(
        seeds = [b"sbtc_mint_authority", squad_multisig.key().as_ref()], 
        bump, // Anchor will find the bump automatically
    )]
    pub sbtc_mint_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for treasury token account
    #[account(
        seeds = [b"treasury_auth_v1", squad_multisig.key().as_ref()], 
        bump, // Anchor will find the bump automatically
    )]
    pub treasury_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for fee token account
    #[account(
        seeds = [b"fee_auth_v1", squad_multisig.key().as_ref()], 
        bump, // Anchor will find the bump automatically
    )]
    pub fee_authority_pda: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct BurnSbtcTest<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// The multisig used as the seed for config/treasury/fees (does not need to be a signer here)
    /// Supply the same squad_multisig that was used during initialize
    /// CHECK: same squad_multisig
    pub squad_multisig: UncheckedAccount<'info>,

    // Config PDA derived from squad_multisig
    #[account(
        mut, // ← Need mut to update total_sbtc_outstanding
        seeds = [b"config_v1", squad_multisig.key().as_ref()],
        bump = config.bump,
        constraint = config.squad_multisig == squad_multisig.key() @ ErrorCode::InvalidSquadMultisig,
    )]
    pub config: Account<'info, Config>,

    // zBTC mint
    pub zbtc_mint: Account<'info, Mint>,

    // sBTC mint
    #[account(mut)]
    pub sbtc_mint: Account<'info, Mint>,

    // User accounts
    #[account(
        mut,
        constraint = user_zbtc_account.mint == zbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_zbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_zbtc_account: Account<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = user_sbtc_account.mint == sbtc_mint.key() @ ErrorCode::InvalidTokenMint,
        constraint = user_sbtc_account.owner == user.key() @ ErrorCode::InvalidTokenOwner,
    )]
    pub user_sbtc_account: Account<'info, TokenAccount>,

    // Treasury vault - NOT a PDA, regular token account
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = treasury_authority_pda,
        constraint = treasury_zbtc_vault.key() == config.treasury_zbtc_vault @ ErrorCode::InvalidTreasuryVault,
    )]
    pub treasury_zbtc_vault: Account<'info, TokenAccount>,

    // Fee vault - NOT a PDA, regular token account  
    #[account(
        mut,
        token::mint = zbtc_mint,
        token::authority = fee_authority_pda,
        constraint = fee_vault.key() == config.fee_vault @ ErrorCode::InvalidFeeVault,
    )]
    pub fee_vault: Account<'info, TokenAccount>,

    /// CHECK: PDA used as authority for treasury token account
    #[account(
        seeds = [b"treasury_auth_v1", squad_multisig.key().as_ref()], 
        bump, // Anchor finds bump automatically
    )]
    pub treasury_authority_pda: UncheckedAccount<'info>,

    /// CHECK: PDA used as authority for fee token account  
    #[account(
        seeds = [b"fee_auth_v1", squad_multisig.key().as_ref()],
        bump, // Anchor finds bump automatically
    )]
    pub fee_authority_pda: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
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
    #[msg("Oracle price invalid")]
    InvalidOraclePrice,
    #[msg("Oracle confidence too large")]
    InvalidOracleConfidence,
    #[msg("Oracle data invalid")]
    InvalidOracle,
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
    #[msg("Stale price data")]
    StaleOraclePrice,
    #[msg("Invalid price value")]
    InvalidPrice,
    #[msg("High confidence interval - unreliable data")]
    HighConfidence,
}
