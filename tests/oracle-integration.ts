import { expect } from 'chai';
import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { OtcSwap } from "../target/types/otc_swap";
import { MockPyth } from "../target/types/mock_pyth";
import { SbtcOracle } from '../target/types/sbtc_oracle';
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  createMint,
  getAccount,
  mintTo,
  createAssociatedTokenAccount,
  TOKEN_PROGRAM_ID,
  getOrCreateAssociatedTokenAccount,
  getMint,
} from "@solana/spl-token";


describe("otc-swap-integration", () => {
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);

  const otcProgram = anchor.workspace.OtcSwap as Program<OtcSwap>;
  const oracleProgram = anchor.workspace.SbtcOracle as Program<SbtcOracle>;
  const mockPyth = anchor.workspace.MockPyth as Program<MockPyth>;

  const connection = provider.connection;
  const admin = provider.wallet;

  // === Test constants ===
  const ZBTC_DECIMALS = 8;
  const SBTC_DECIMALS = 8;
  const FEE_RATE_BPS = 500; // 5%
  const MIN_COLLATERAL_BPS = 20000; // 200%

  const INITIAL_PRICE = new BN(10_000_000_000_000); // $100,000 in Pyth format
  const UPDATED_PRICE = new BN(12_500_000_000_000);
  const INITIAL_CONF = new BN(500);
  const UPDATED_CONF = new BN(600);
  const PRICE_EXPO = -8;

  const NEW_ORACLE_TREND = new BN(10_000_000);  // $100,000 in cents (100,000 * 100)

  // === Accounts ===
  let sbtcMint: anchor.web3.PublicKey;
  let zbtcMint: anchor.web3.PublicKey;

  let sbtcMintAuthorityPda: anchor.web3.PublicKey;
  let treasuryAuthorityPda: anchor.web3.PublicKey;
  let feeAuthorityPda: anchor.web3.PublicKey;
  let configPda: anchor.web3.PublicKey;

  let treasuryZbtcVault: anchor.web3.PublicKey;
  let feeVault: anchor.web3.PublicKey;

  let oracleStatePda: anchor.web3.PublicKey;
  let pythPriceAccount: anchor.web3.Keypair;
  let pythPriceFeed: anchor.web3.PublicKey;

  before(async () => {
    // === Derive PDAs ===
    [configPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("config_v1"), admin.publicKey.toBuffer()],
      otcProgram.programId
    );

    [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("sbtc_mint_authority"), admin.publicKey.toBuffer()],
      otcProgram.programId
    );

    [treasuryAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("treasury_auth_v1"), admin.publicKey.toBuffer()],
      otcProgram.programId
    );

    [feeAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fee_auth_v1"), admin.publicKey.toBuffer()],
      otcProgram.programId
    );

    [oracleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("oracle")],
      oracleProgram.programId
    );
  
    console.log(`configPda:${configPda} sbtcMintAuthorityPda:${sbtcMintAuthorityPda} treasuryAuthorityPda:${treasuryAuthorityPda} feeAuthorityPda:${feeAuthorityPda}`);

    // === Create mints ===
    zbtcMint = await createMint(connection, admin.payer, admin.publicKey, null, ZBTC_DECIMALS);
    sbtcMint = await createMint(connection, admin.payer, admin.publicKey, admin.publicKey, SBTC_DECIMALS);
    console.log(`zbtcMint:${zbtcMint} sbtcMint:${sbtcMint}`);

    // === Create token accounts ===
    const treasuryAccount = await getOrCreateAssociatedTokenAccount(
        connection,
        admin.payer,
        zbtcMint,
        treasuryAuthorityPda, // PDA as owner
        true, // allowOwnerOffCurve - for PDAs
        undefined, // confirmOptions
        undefined, // programId (uses TOKEN_PROGRAM_ID)
        TOKEN_PROGRAM_ID // explicitly specify token program
    );
    treasuryZbtcVault = treasuryAccount.address;

    const feeAccount = await getOrCreateAssociatedTokenAccount(
        connection,
        admin.payer,
        zbtcMint,
        feeAuthorityPda,
        true,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID
    );
    feeVault = feeAccount.address;
    console.log(`treasuryZbtcVault:${treasuryZbtcVault} feeVault:${feeVault}`);

    // === Create and initialize mock Pyth price account ===
    pythPriceAccount = Keypair.generate();
    const PRICE_ACCOUNT_SIZE = 3312;
    const lamports = await connection.getMinimumBalanceForRentExemption(PRICE_ACCOUNT_SIZE);

    const createIx = SystemProgram.createAccount({
      fromPubkey: admin.publicKey,
      newAccountPubkey: pythPriceAccount.publicKey,
      lamports,
      space: PRICE_ACCOUNT_SIZE,
      programId: mockPyth.programId,
    });

    await provider.sendAndConfirm(
      new anchor.web3.Transaction().add(createIx),
      [pythPriceAccount, admin.payer]
    );

    // === Initialize the price feed ===
    await mockPyth.methods
      .initialize(INITIAL_PRICE, PRICE_EXPO, INITIAL_CONF)
      .accounts({
        price: pythPriceAccount.publicKey,
      })
      .rpc();

    pythPriceFeed = pythPriceAccount.publicKey;
    console.log(`✅ Mock Pyth feed created: ${pythPriceFeed}`);

    await oracleProgram.methods
    .initialize()
    .accounts({
      oracleState: oracleStatePda,
      authority: admin.publicKey,
      systemProgram: SystemProgram.programId,
    } as any)
    .rpc();

    console.log("Oracle initialized.");

    const tx = await oracleProgram.methods
      .updateTrendTest(NEW_ORACLE_TREND)
      .accounts({
        oracleState: oracleStatePda,
        authority: admin.publicKey,
      } as any)
      .rpc();
    
      console.log("oracle updated.");

    const oracleState = await oracleProgram.account.oracleState.fetch(oracleStatePda);
    console.log("Oracle trend_value:", oracleState.trendValue.toString());
  });

  it("initialize", async () => {
    console.log("AYO INITIALIZE");

    // === Verify sBTC initially owned by admin (squad) ===
    let mintInfo = await getMint(connection, sbtcMint);
    expect(mintInfo.mintAuthority?.equals(admin.publicKey)).to.be.true;

    const tx = await otcProgram.methods
      .initialize(
        new anchor.BN(FEE_RATE_BPS),
        new anchor.BN(MIN_COLLATERAL_BPS),
      )
      .accounts({
        squadMultisig: admin.publicKey,
        sbtcMint: sbtcMint,
        zbtcMint: zbtcMint,
        sbtcMintAuthorityPda: sbtcMintAuthorityPda,
        treasuryAuthorityPda: treasuryAuthorityPda,
        feeAuthorityPda: feeAuthorityPda,
        treasuryZbtcVault: treasuryZbtcVault,
        feeVault: feeVault,
        config: configPda,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      } as any)
      .rpc();

    console.log("Initialize tx:", tx);

    // === Verify sBTC authority was transferred to program PDA ===
    mintInfo = await getMint(connection, sbtcMint);
    expect(mintInfo.mintAuthority?.equals(sbtcMintAuthorityPda)).to.be.true;

    // === Verify vaults are correct ===
    const treasuryAccount = await getAccount(connection, treasuryZbtcVault);
    const feeAccount = await getAccount(connection, feeVault);
    expect(treasuryAccount.owner.equals(treasuryAuthorityPda)).to.be.true;
    expect(feeAccount.owner.equals(feeAuthorityPda)).to.be.true;
    expect(treasuryAccount.mint.equals(zbtcMint)).to.be.true;
    expect(feeAccount.mint.equals(zbtcMint)).to.be.true;

    // === Verify config was stored ===
    const config = await otcProgram.account.config.fetch(configPda);
    expect(config.squadMultisig.equals(admin.publicKey)).to.be.true;
    expect(config.sbtcMint.equals(sbtcMint)).to.be.true;
    expect(config.zbtcMint.equals(zbtcMint)).to.be.true;
    expect(config.treasuryZbtcVault.equals(treasuryZbtcVault)).to.be.true;
    expect(config.feeVault.equals(feeVault)).to.be.true;
    expect(config.feeRateBps.toNumber()).to.equal(FEE_RATE_BPS);
    expect(config.minCollateralBps.toNumber()).to.equal(MIN_COLLATERAL_BPS);
    expect(config.paused).to.be.false;
    expect(config.totalSbtcOutstanding.toString()).to.equal("0");
  });

  it("mint", async () => {
    console.log("AYO MINT");

    // === Create user ===
    let user = Keypair.generate();
    console.log(`user:${user.publicKey.toBase58()}`);
    await connection.requestAirdrop(user.publicKey, 1e9);

    // === Create token accounts ===
    let userZbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey);
    let userSbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey);
    console.log(`userZbtcAccount:${userZbtcAccount} userSbtcAccount:${userSbtcAccount}`);

    // === Fund user with zBTC ===
    await mintTo(connection, admin.payer, zbtcMint, userZbtcAccount, admin.publicKey, 10_000_000_000);

    // === Fund treasury with zBTC ===
    await mintTo(connection, admin.payer, zbtcMint, treasuryZbtcVault, admin.publicKey, 10_000_000_000);

    // === Update mock Pyth price ===
    await mockPyth.methods
    .setPrice(UPDATED_PRICE, UPDATED_CONF)
    .accounts({
      price: pythPriceFeed,
      authority: admin.publicKey,
    } as any)
    .rpc();

    console.log("✅ Mock Pyth price updated.");

    const deposit = new anchor.BN(100_000_000); // (8 decimals)
    const fee = deposit.toNumber() * FEE_RATE_BPS / 10_000;
    const netDeposit = deposit.toNumber() - fee;

    // Pre balances
    const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
    const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
    const preTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
    const preFee = (await getAccount(connection, feeVault)).amount;
    const preConfig = await otcProgram.account.config.fetch(configPda);

    console.log("Pre-mint balances:");
    console.log("User zBTC:", preUserZbtc.toString());
    console.log("User sBTC:", preUserSbtc.toString());
    console.log("Treasury:", preTreasury.toString());
    console.log("Fee vault:", preFee.toString());
    console.log("Total sBTC outstanding:", preConfig.totalSbtcOutstanding.toString());

    const tx = await otcProgram.methods
    .mintSbtc(deposit)
    .accounts({
      user: user.publicKey,
      squadMultisig: admin.publicKey,
      config: configPda,
      sbtcMint: sbtcMint,
      zbtcMint: zbtcMint,
      userSbtcAccount: userSbtcAccount,
      userZbtcAccount: userZbtcAccount,
      treasuryZbtcVault: treasuryZbtcVault,
      feeVault: feeVault,
      sbtcMintAuthorityPda: sbtcMintAuthorityPda,
      treasuryAuthorityPda: treasuryAuthorityPda,
      feeAuthorityPda: feeAuthorityPda,
      pythPriceAccount: pythPriceFeed,
      oracleState: oracleStatePda,
      tokenProgram: TOKEN_PROGRAM_ID,
    } as any)
    .signers([user])
    .rpc();

    console.log("✅ sBTC minted successfully, tx:", tx);

    // === Post balances ===
    const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
    const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
    const postTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
    const postFee = (await getAccount(connection, feeVault)).amount;
    const postConfig = await otcProgram.account.config.fetch(configPda);

    console.log("Post-mint balances:");
    console.log("User zBTC:", postUserZbtc.toString());
    console.log("User sBTC:", postUserSbtc.toString());
    console.log("Treasury:", postTreasury.toString());
    console.log("Fee vault:", postFee.toString());
    console.log("Total sBTC outstanding:", postConfig.totalSbtcOutstanding.toString());

    // === Assertions ===
    expect(postUserZbtc.toString()).to.equal((Number(preUserZbtc) - deposit.toNumber()).toString());
    expect(postTreasury.toString()).to.equal((Number(preTreasury) + netDeposit).toString());
    expect(postFee.toString()).to.equal((Number(preFee) + fee).toString());
    expect(postUserSbtc > preUserSbtc).to.be.true;

    // === Config check ===
    const config = await otcProgram.account.config.fetch(configPda);
    expect(config.totalSbtcOutstanding.toString()).to.equal(postUserSbtc.toString());
  });

  it("burn", async () => {
    console.log("AYO BURN");

    // === Create user ===
    let user = Keypair.generate();
    console.log(`user:${user.publicKey.toBase58()}`);
    await connection.requestAirdrop(user.publicKey, 1e9);

    // === Create token accounts ===
    let userZbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey);
    let userSbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey);
    console.log(`userZbtcAccount:${userZbtcAccount} userSbtcAccount:${userSbtcAccount}`);

    // === Fund user with zBTC ===
    await mintTo(connection, admin.payer, zbtcMint, userZbtcAccount, admin.publicKey, 10_000_000_000);

    // === Fund treasury with zBTC ===
    await mintTo(connection, admin.payer, zbtcMint, treasuryZbtcVault, admin.publicKey, 10_000_000_000);

    // === Update mock Pyth price ===
    await mockPyth.methods
    .setPrice(UPDATED_PRICE, UPDATED_CONF)
    .accounts({
      price: pythPriceFeed,
      authority: admin.publicKey,
    } as any)
    .rpc();

    console.log("✅ Mock Pyth price updated.");

    const zbtcAmount = new anchor.BN(100_000_000); // (8 decimals)
    let fee = zbtcAmount.toNumber() * FEE_RATE_BPS / 10_000;

    const mintTx = await otcProgram.methods
    .mintSbtc(zbtcAmount)
    .accounts({
      user: user.publicKey,
      squadMultisig: admin.publicKey,
      config: configPda,
      sbtcMint: sbtcMint,
      zbtcMint: zbtcMint,
      userSbtcAccount: userSbtcAccount,
      userZbtcAccount: userZbtcAccount,
      treasuryZbtcVault: treasuryZbtcVault,
      feeVault: feeVault,
      sbtcMintAuthorityPda: sbtcMintAuthorityPda,
      treasuryAuthorityPda: treasuryAuthorityPda,
      feeAuthorityPda: feeAuthorityPda,
      pythPriceAccount: pythPriceFeed,
      oracleState: oracleStatePda,
      tokenProgram: TOKEN_PROGRAM_ID,
    } as any)
    .signers([user])
    .rpc();

    console.log("✅ sBTC minted successfully, tx:", mintTx);

    const burnAmount = new anchor.BN(50_000_000); // 0.5 sBTC
    const sbtcToBurn = burnAmount.toNumber() / Math.pow(10, SBTC_DECIMALS);
    const sbtcPrice = NEW_ORACLE_TREND.toNumber() / 100;
    const discount = (10_000 - FEE_RATE_BPS) / 10_000;
    const zbtcPrice = UPDATED_PRICE.toNumber() / Math.pow(10, ZBTC_DECIMALS);
    const totZbtc = (sbtcToBurn * sbtcPrice / zbtcPrice) * Math.pow(10, ZBTC_DECIMALS);
    const netZbtc = totZbtc * discount;
    fee = totZbtc - netZbtc;

    console.log(`${sbtcToBurn} * ${sbtcPrice} * ${discount} / ${zbtcPrice} `);
    console.log(`netZbtc:${netZbtc}`);
    console.log(`burn fee:${fee}`);

    // === Pre balances ===
    const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
    const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
    const preTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
    const preFee = (await getAccount(connection, feeVault)).amount;
    const preConfig = await otcProgram.account.config.fetch(configPda);

    console.log("Pre-burn balances:");
    console.log("User zBTC:", preUserZbtc.toString());
    console.log("User sBTC:", preUserSbtc.toString());
    console.log("Treasury:", preTreasury.toString());
    console.log("Fee vault:", preFee.toString());
    console.log("Total sBTC outstanding:", preConfig.totalSbtcOutstanding.toString());

    try{ 
      const burnTx = await otcProgram.methods
      .burnSbtc(burnAmount)
      .accounts({
        user: user.publicKey,
        squadMultisig: admin.publicKey,
        config: configPda,
        sbtcMint: sbtcMint,
        zbtcMint: zbtcMint,
        userSbtcAccount: userSbtcAccount,
        userZbtcAccount: userZbtcAccount,
        treasuryZbtcVault: treasuryZbtcVault,
        feeVault: feeVault,
        treasuryAuthorityPda: treasuryAuthorityPda,
        feeAuthorityPda: feeAuthorityPda,
        pythPriceAccount: pythPriceFeed,
        oracleState: oracleStatePda,
        tokenProgram: TOKEN_PROGRAM_ID,
      } as any)
      .signers([user])
      .rpc();

      console.log("✅ sBTC burned successfully, tx:", burnTx);

      // === Post balances ===
      const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
      const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
      const postTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
      const postFee = (await getAccount(connection, feeVault)).amount;
      const postConfig = await otcProgram.account.config.fetch(configPda);

      console.log("Post-burn balances:");
      console.log("User zBTC:", postUserZbtc.toString());
      console.log("User sBTC:", postUserSbtc.toString());
      console.log("Treasury:", postTreasury.toString());
      console.log("Fee vault:", postFee.toString());
      console.log("Total sBTC outstanding:", postConfig.totalSbtcOutstanding.toString());

      // === Assertions ===
      // User sBTC should be burned
      expect(Number(postUserSbtc)).to.equal(Number(preUserSbtc) - burnAmount.toNumber());
      
      // User should receive net zBTC (after fee)
      expect(Number(postUserZbtc)).to.equal(Number(preUserZbtc) + netZbtc);
      
      // Treasury should decrease by total zBTC value (net + fee)
      expect(Number(postTreasury)).to.equal(Number(preTreasury) - (netZbtc + fee));
      
      // Fee vault should increase by fee amount
      expect(Number(postFee)).to.equal(Number(preFee) + fee);

      // Total sBTC outstanding should decrease
      expect(postConfig.totalSbtcOutstanding.toString()).to.equal(
        (Number(preConfig.totalSbtcOutstanding) - burnAmount.toNumber()).toString()
      );
    }
    catch(e) {
      console.error("Error while burning.");
      console.error(e);
      throw e;
    }
  });
});
