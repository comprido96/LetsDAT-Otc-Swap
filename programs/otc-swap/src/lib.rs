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
// use pyth_client::{Price as PythPrice, cast};


declare_id!("BkCS4J3v38Pfr5U37qXqdGfJr7YDHKrKutdctJg1caxv");


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

        // paused check
        require!(!config.paused, ErrorCode::Paused);
    
        // ownership sanity (config checks above assert many things; extra cross-checks are defensive)
        require!(ctx.accounts.zbtc_mint.key() == config.zbtc_mint, ErrorCode::InvalidZbtcMint);
        require!(ctx.accounts.sbtc_mint.key() == config.sbtc_mint, ErrorCode::InvalidSbtcMint);

        // user owns the token accounts (defensive)
        require!(ctx.accounts.user_zbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);
        require!(ctx.accounts.user_sbtc_account.owner == ctx.accounts.user.key(), ErrorCode::InvalidTokenAccountOwner);

        // ensure user has sufficient balance
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

        // -- 3) read & validate oracle price (Pyth-style)
        // Here we parse the pyth price account; adapt to your oracle's exact API if different.
        // We'll interpret the oracle price as "zBTC per 1 sBTC" scaled by `10^{expo}` (pyth convention).
        // So numeric price = price.i64 * 10^{expo} (expo may be negative).
        // let (price_num_i128, price_expo_i32, conf_i128, last_slot) = {
        //     // Use pyth_client::cast to parse; adapt if you're using another parity.
        //     let data = ctx.accounts.price_account.try_borrow_data()?;
        //     // Safety: pyth_client::cast requires exact bytes layout. If you use a different oracle,
        //     // replace these lines with the appropriate parser.
        //     let pyth_price: &PythPrice = pyth_client::cast::<PythPrice>(&data);
        //     // pyth Price fields: price.unix_timestamp, price.agg.price (i64), price.agg.conf (u64), price.expo (i32)
        //     let price_i64 = pyth_price.agg.price;
        //     let conf = pyth_price.agg.conf as i128;
        //     let expo = pyth_price.expo;
        //     let slot = pyth_price.valid_slot; // or pyth_price.px_acc; adapt as needed
        //     (price_i64 as i128, expo, conf, slot)
        // };
        

        // // Basic oracle checks: price not zero, confidence not too large, and recent slot (optionally)
        // require!(price_num_i128 != 0, ErrorCode::InvalidOraclePrice);
        // // Optional: enforce a maximum relative confidence ratio, e.g. conf/price < 1%:
        // // compute conf_ratio = conf_i128 * 10_000 / abs(price_num_i128) // in basis points
        // let abs_price = if price_num_i128 < 0 { -price_num_i128 } else { price_num_i128 };
        // // avoid division by zero (we already checked price != 0)
        // let conf_bps = (conf_i128
        //     .checked_mul(10_000)
        //     .ok_or(ErrorCode::InvalidOraclePrice)?
        //     .checked_div(abs_price)
        //     .ok_or(ErrorCode::InvalidOraclePrice)?) as i128;
        // // require conf_bps <= some threshold (e.g., 2000 bps => 20%). Choose conservative threshold.
        // require!(conf_bps <= 5000, ErrorCode::InvalidOracleConfidence); // <=50% conf (example)

        // -- 4) compute how many sBTC units to mint for `net_zbtc_u128` at this price,
        // accounting for decimals

        // We'll compute everything in "minor units" (i.e., token base units).
        // Notation:
        //  - net_zbtc_u128 is in zBTC minor units (e.g., if zBTC decimals = 8, 1 zBTC = 10^8)
        //  - sBTC to mint (minor units) = net_zbtc_value_in_zbtc_units * (10^{sbtc_decimals}) / price_scaled_to_zbtc_per_1_sbtc

        // Construct integer `price_scale` = 10^{|expo|}. price_value = price_num_i128 * (if expo >=0 then price_scale else 1/price_scale)
        // To avoid fractions, we'll rearrange:
        // sbtc_to_mint = net_zbtc * (10^sbtc_decimals) * (10^{max(0, -expo)}) / (abs(price_num) * 10^{max(0, expo)})
        // Simplified approach:
        // Let price = price_num_i128 * 10^{expo}  (a rational number)
        // Let Z = net_zbtc (in z-minor-units)
        // 1 sBTC nominal corresponds to `price` zBTC (price may be fractional).
        // sBTC_minor_units = (Z * 10^{sbtc_decimals}) / price

        // To do integer math:
        // numerator = Z * 10^{sbtc_decimals} * 10^{(-expo) if expo < 0 else 0}
        // denominator = price_num_i128 * 10^{(expo) if expo > 0 else 0}
        // (this keeps integers)

        // let sbtc_decimals = config.sbtc_decimals as i32;
        // let zbtc_decimals = config.zbtc_decimals as i32;
        // let expo = price_expo_i32;

        // // Build numerator and denominator as u128 where possible (convert sign-aware)
        // // numerator = net_zbtc_u128 * 10^{sbtc_decimals} * 10^{neg_expo}
        // let neg_expo = if expo < 0 { (-expo) as usize } else { 0usize };
        // let pos_expo = if expo > 0 { expo as usize } else { 0usize };

        // // compute pow10 factors; check overflow risk (exponents should be small for Pyth; sanity cap)
        // // cap exponent growth for safety
        // require!(neg_expo <= 38 && pos_expo <= 38, ErrorCode::InvalidOraclePrice); // Pyth exponents are tiny, this guards weird data

        // let pow_sbtc_dec = 10u128.checked_pow(sbtc_decimals as u32).ok_or(ErrorCode::InvalidAmount)?;
        // let pow_neg_expo = if neg_expo > 0 { 10u128.checked_pow(neg_expo as u32).ok_or(ErrorCode::InvalidAmount)? } else { 1u128 };
        // let pow_pos_expo = if pos_expo > 0 { 10u128.checked_pow(pos_expo as u32).ok_or(ErrorCode::InvalidAmount)? } else { 1u128 };

        // // numerator = net_zbtc_u128 * pow_sbtc_dec * pow_neg_expo
        // let numerator = net_zbtc_u128
        //     .checked_mul(pow_sbtc_dec).ok_or(ErrorCode::InvalidAmount)?
        //     .checked_mul(pow_neg_expo).ok_or(ErrorCode::InvalidAmount)?;

        // // denominator = abs(price_num_i128 as u128) * pow_pos_expo
        // let price_abs_u128 = if price_num_i128 < 0 { (-price_num_i128) as u128 } else { price_num_i128 as u128 };
        // let denominator = price_abs_u128.checked_mul(pow_pos_expo).ok_or(ErrorCode::InvalidAmount)?;

        // // finally, sbtc_to_mint in minor units:
        // let sbtc_to_mint_u128 = numerator.checked_div(denominator).ok_or(ErrorCode::InvalidAmount)?;
        // require!(sbtc_to_mint_u128 > 0, ErrorCode::InvalidAmount);

        // // For Solana token CPI we often need u64 amounts. If sBTC decimals and expected supply fit u64, cast.
        // // Choose to keep sBTC mint amounts as u128 for accounting, but we must pass a u64 to token::mint_to.
        // // Ensure amount fits in u64
        // require!(sbtc_to_mint_u128 <= u64::MAX as u128, ErrorCode::InvalidAmount);
        // let sbtc_to_mint_u64 = sbtc_to_mint_u128 as u64;

        let sbtc_to_mint_u64 = get_sbtc_price()? * net_zbtc_u64;

        // -- 5) Transfer net_zbtc -> treasury_vault and fee -> fee_vault (signed by user)
        // net_zbtc_u64 and fee_amount_u64 are the amounts to transfer from user to PDAs
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

        // -- 6) Mint sBTC to user using the sbtc_mint_authority PDA as signer
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

        // -- 7) Update accounting: total_sbtc_outstanding
        // total_sbtc_outstanding stored in minor units u128
        config.total_sbtc_outstanding = config.total_sbtc_outstanding
            .checked_add(sbtc_to_mint_u64 as u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        // -- 8) Collateral check (post-mint)
        // Required zBTC collateral = total_sbtc_outstanding (in sBTC minor units) * price / 10^{sbtc_decimals}
        // We'll compute required_zbtc_minor_units = ceil(total_sbtc_outstanding * price_scaled / 10^{sbtc_decimals})
        // Using integer math similar to above:
        // required_zbtc = total_sbtc_outstanding * (price_num * 10^{pos_expo}) / (10^{sbtc_decimals} * 10^{neg_expo})
        // To compare to treasury balance (which is in zbtc minor units), rearrange to same units.

        // // Build numerator2 = total_sbtc_outstanding * price_abs_u128 * pow_pos_expo
        // let total_sbtc = config.total_sbtc_outstanding; // u128
        // let numerator2 = total_sbtc
        //     .checked_mul(price_abs_u128).ok_or(ErrorCode::InvalidAmount)?
        //     .checked_mul(pow_pos_expo).ok_or(ErrorCode::InvalidAmount)?;

        // // denominator2 = pow_sbtc_dec * pow_neg_expo
        // let denominator2 = pow_sbtc_dec.checked_mul(pow_neg_expo).ok_or(ErrorCode::InvalidAmount)?;

        // let required_zbtc_minor = numerator2.checked_div(denominator2).ok_or(ErrorCode::InvalidAmount)?;
        // // Apply min_collateral_bps buffer: required_zbtc_with_buffer = required_zbtc_minor * (min_collateral_bps) / 10_000
        // let min_collateral_bps = config.min_collateral_bps as u128; // e.g., 20_000 = 200%
        // let required_zbtc_with_buffer = required_zbtc_minor
        //     .checked_mul(min_collateral_bps).ok_or(ErrorCode::InvalidAmount)?
        //     .checked_div(10_000u128).ok_or(ErrorCode::InvalidAmount)?;

        // // Check treasury balance (in minor units)
        // let treasury_balance = ctx.accounts.treasury_zbtc_vault.amount as u128;

        // require!(treasury_balance >= required_zbtc_with_buffer, ErrorCode::InsufficientCollateral);

        // -- 9) Emit event
        emit!(MintEvent {
            user: ctx.accounts.user.key(),
            zbtc_deposited: zbtc_amount,
            sbtc_minted: sbtc_to_mint_u64 as u128,
            fee_amount: fee_amount_u64,
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

        // Oracle (placeholder: 1:1)
        let sbtc_price = get_sbtc_price()?; 
        let zbtc_value = sbtc_amount
            .checked_mul(sbtc_price)
            .ok_or(ErrorCode::InvalidAmount)?;
        
        // Fee
        let fee = zbtc_value
            .checked_mul(config.fee_rate_bps as u64)
            .ok_or(ErrorCode::InvalidAmount)?
            .checked_div(10_000)
            .ok_or(ErrorCode::InvalidAmount)?;
        let net_zbtc = zbtc_value.checked_sub(fee).ok_or(ErrorCode::InvalidAmount)?;
        require!(net_zbtc > 0, ErrorCode::InvalidAmount);

        // Treasury liquidity
        require!(ctx.accounts.treasury_zbtc_vault.amount >= zbtc_value, ErrorCode::InsufficientBalance);

        // Burn sBTC
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

        // Transfer net redemption
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
            net_zbtc,
        )?;

        // Transfer fee
        if fee > 0 {
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
                fee,
            )?;
        }

        // Update accounting
        config.total_sbtc_outstanding = config.total_sbtc_outstanding
            .checked_sub(sbtc_amount as u128)
            .ok_or(ErrorCode::InvalidAmount)?;

        // Optional: collateral check again here (like in mint)

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
fn get_sbtc_price() -> Result<u64> {
    // TODO: Replace with actual oracle CPI call
    // For now, return hardcoded 1:1 price
    Ok(1)
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
        init,
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
    pub price_account: UncheckedAccount<'info>,

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
    #[msg("Oracle price invalid")]
    InvalidOraclePrice,
    #[msg("Oracle confidence too large")]
    InvalidOracleConfidence,
    #[msg("Oracle data invalid")]
    InvalidOracle,
    #[msg("Protocol paused")]
    Paused,
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
}


/*
NOTES
1) Authority PDAs (treasury_authority_pda) are the signer for future CPIs (e.g., to transfer from vault),
and unlike the token account itself they can be derived and used with signer seeds.
Setting token::authority to the token account key (as in your original code)
prevents you from ever using PDA signer seeds to sign for transfers — which breaks treasury operations.

2) The authority bumps stored in config can later be used to produce signer seeds:
let seeds = &[b"treasury_auth", squad_multisig.key().as_ref(), &[config.treasury_auth_bump]];

3) MATH - Use u128 for multiply-before-divide.
4) MATH - Normalize token decimals explicitly (e.g., zBTC has zbtc_decimals, sBTC has sbtc_decimals).
5) MATH - Represent BPS (basis points) with denominator 10_000
Example:
// math utils (in Rust inside anchor program)
fn checked_mul_div_floor(a: u128, b: u128, d: u128) -> Result<u128> {
    a.checked_mul(b).ok_or(ErrorCode::InvalidAmount)?.checked_div(d).ok_or(ErrorCode::InvalidAmount)
}

// Compute required ZBTC collateral for `sbtc_amount` of sBTC tokens (both in their token units)
// price_zbtc_per_btc: price oracle output represented as zBTC per 1 BTC scaled by oracle_scale
// sbtc_to_btc_ratio: if 1 sBTC == 1 BTC nominal; else adjust.
fn required_collateral_zbtc_for_sbtc(
    sbtc_amount: u128, // in sBTC-minor-units (e.g., satoshi-equivalents)
    sbtc_decimals: u8,
    zbtc_decimals: u8,
    price_zbtc_per_btc_scaled: u128, // careful with Pyth exponent
    price_scale: u128, // e.g., 10^price_exponent_abs
    min_collateral_bps: u128, // where 10_000 == 100%
) -> Result<u128> {
    // convert sbtc_amount to BTC units (both decimals)
    // For 1:1 peg, sbtc_amount * price_zbtc_per_btc_scaled / price_scale = zBTC amount needed (in zbtc-minor-units)
    // then apply margin (min_collateral_bps)
    // Formula: zbtc_needed = sbtc_amount * price_zbtc_per_btc_scaled / price_scale
    let zbtc_needed = checked_mul_div_floor(sbtc_amount, price_zbtc_per_btc_scaled, price_scale)?;
    let zbtc_with_buffer = checked_mul_div_floor(zbtc_needed, min_collateral_bps + 10_000, 10_000)?; // adds min_collateral_bps percent
    Ok(zbtc_with_buffer)
}

6) GOVERNANCE - Admin-only instructions (signed by squad_multisig): set_fee_rate, set_min_collateral_bps, pause, unpause, withdraw_fees,
withdraw_excess_treasury, set_yield_strategy. Require multi-sig as signer.
7) GOVERNANCE - Pause & emergency withdraw: allow the multisig to pause the protocol (disable mints/burns) and withdraw fees or
migrate treasury in a controlled way. Emit events for these ops.
8) GOVERNANCE - On-chain accounting: track total_sbtc_issued and total_zbtc_in_treasury in Config or a separate accounting account for easier checks.
Keep historical snapshots or events for auditing.
9) GOVERNANCE - Periodic rebalancing: implement rebalancing ops for treasury yield strategies; only multisig may call. When treasury transfers tokens
to external yield strategies, mark them as illiquid and exclude from immediate collateral for redemptions
10) GOVERNANCE - Fees: clarify whether fee is taken from deposit (zBTC) or from minted sBTC amount. Decide and document (and implement accordingly).
Collect fees in zBTC for simplicity.

11) TESTING/SECURITY - Unit tests for init, mint, burn, and governance flows.
12) TESTING/SECURITY - Property/fuzz tests for arithmetic (boundary amounts, overflow), decimal conversion.
13) TESTING/SECURITY - Integration tests on Devnet using your Pyth SMA oracle (verify price scale conversions).
14) TESTING/SECURITY - Simulate churn / oracle staleness: ensure stale oracle causes rejects.
15) TESTING/SECURITY - Slippage and rounding tests: ensure no rounding drains (e.g., repeated tiny mints & burns should not grief treasury).
16) TESTING/SECURITY - Audits & formal review: get a third-party audit for the economic logic, PDAs, authority flows, and CPI signer usage.

17) MISC - Use descriptive PDA seeds that include the squad_multisig key and a unique label (e.g., b"treasury_auth_v1").
That helps future upgrades and prevents silent collisions.
18) MISC - Add InitializedEvent (you already have it) but also emit events on fee-changes, mints, burns, transfers to yield strategies.
19) MISC - Be explicit about decimals in the Config (store zbtc_decimals and sbtc_decimals) so code doesn’t rely on external assumptions.
20) MISC - when you issue CPIs that use PDA signer seeds, use the bumps you stored; do not recompute via brute force—read bumps from Config.
21) MISC - Emit events with amounts on mint/burn so off-chain indexers can reconstruct supply and fee flows.
22) MISC - Add withdraw_fees instruction (multisig-only) to move tokens from fee_vault to a multisig-managed account.
23) MISC - Ensure any external yield strategy transfers are tracked (liquid vs illiquid) so redemptions cannot drain illiquid funds.


TODOS
1) Collateral math & decimals: need to be explicit about token decimals and oracle price scale.
Use u128 for arithmetic, normalize decimals, and avoid remainders / rounding attacks.

2) Oracle usage: you referenced a Pyth-based SMA oracle off-chain; ensure you
(a) validate oracle freshness, (b) handle price confidence, (c) normalize price scale (Pyth uses exponent).

3) Governance & admin ops: allow only squad_multisig (config.squad_multisig) to change fees/min-collateral,
and add pausing and emergency withdrawal with multisig-only ops.

4) Init authority/semantics: when checking initial sbtc mint authority, comparing COption must be done safely.
The code currently uses Some(...).into() which compiles often but be clear and explicit.
*/
