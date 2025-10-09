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
} from "@solana/spl-token";
import { program } from '@coral-xyz/anchor/dist/cjs/native/system';


describe("otc-swap-integration", () => {
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);

  const otcProgram = anchor.workspace.OtcSwap as Program<OtcSwap>;
  const oracleProgram = anchor.workspace.SbtcOracle as Program<SbtcOracle>;
  const mockPyth = anchor.workspace.MockPyth as Program<MockPyth>;

  const connection = provider.connection;
  const admin = provider.wallet;

  // Test constants
  const ZBTC_DECIMALS = 8;
  const SBTC_DECIMALS = 8;
  const FEE_RATE_BPS = 500; // 5%
  const MIN_COLLATERAL_BPS = 20000; // 200%

  const INITIAL_PRICE = new BN(12_500_000_000); // $125,000 = 12500000000 * 10^-8
  const INITIAL_CONF = new BN(500);
  const UPDATED_PRICE = new BN(140_000_000); // $140,000
  const UPDATED_CONF = new BN(600);
  const PRICE_EXPO = -8; // Standard Pyth exponent for BTC/USD


  // Mock prices (in cents)
  const ZBTC_PRICE = new BN(12_500_000); // $125,000
  const SBTC_PRICE = new BN(10_000_000); // $100,000

  // Accounts
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
    // Derive PDAs
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

    // Create mints
    zbtcMint = await createMint(connection, admin.payer, admin.publicKey, null, ZBTC_DECIMALS);
    sbtcMint = await createMint(connection, admin.payer, admin.publicKey, admin.publicKey, SBTC_DECIMALS);
    console.log(`zbtcMint:${zbtcMint} sbtcMint:${sbtcMint}`);

    // === Create token accounts ===
    const treasuryAccount = await getOrCreateAssociatedTokenAccount(
        connection,
        admin.payer,
        zbtcMint,
        treasuryAuthorityPda, // PDA as owner
        true, // allowOwnerOffCurve - IMPORTANT for PDAs
        undefined, // confirmOptions
        undefined, // programId (uses TOKEN_PROGRAM_ID)
        TOKEN_PROGRAM_ID // explicitly specify token program
    );
    treasuryZbtcVault = treasuryAccount.address;

    const feeAccount = await getOrCreateAssociatedTokenAccount(
        connection,
        admin.payer,
        zbtcMint,
        feeAuthorityPda, // PDA as owner
        true, // allowOwnerOffCurve - IMPORTANT for PDAs
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

    // Initialize the price feed
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

    const newTrend = new BN(50000);

    const tx = await oracleProgram.methods
      .updateTrendTest(newTrend)
      .accounts({
        oracleState: oracleStatePda,
        authority: admin.publicKey,
      } as any)
      .rpc();
    
      console.log("oracle updated.");
  });

  it("initialize", async () => {
    console.log("AYO INITIALIZE");

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
  });

  it("mint", async () => {
    console.log("AYO MINT");

    // Create user
    let user = Keypair.generate();
    console.log(`user:${user.publicKey.toBase58()}`);
    await connection.requestAirdrop(user.publicKey, 1e9);

    // Create token accounts
    let userZbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey);
    let userSbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey);
    console.log(`userZbtcAccount:${userZbtcAccount} userSbtcAccount:${userSbtcAccount}`);

    // Fund user with zBTC
    await mintTo(connection, admin.payer, zbtcMint, userZbtcAccount, admin.publicKey, 10_000_000_000);

    // Fund treasury with zBTC
    await mintTo(connection, admin.payer, zbtcMint, treasuryZbtcVault, admin.publicKey, 10_000_000_000);

    // Update mock Pyth price
    await mockPyth.methods
    .setPrice(UPDATED_PRICE, UPDATED_CONF)
    .accounts({
      price: pythPriceFeed,
      authority: admin.publicKey,
    } as any)
    .rpc();

  console.log("✅ Mock Pyth price updated to $140,000");

    const zbtcAmount = new anchor.BN(100_000_000); // 1 zBTC (8 decimals)
    const fee = zbtcAmount.toNumber() * FEE_RATE_BPS / 10_000; // 5% = 0.05 zBTC
    const netDeposit = zbtcAmount.toNumber() - fee;

    const tx = await otcProgram.methods
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

    console.log("✅ sBTC minted successfully, tx:", tx);
  });

  it("burn", async () => {
    console.log("AYO BURN");

    // Create user
    let user = Keypair.generate();
    console.log(`user:${user.publicKey.toBase58()}`);
    await connection.requestAirdrop(user.publicKey, 1e9);

    // Create token accounts
    let userZbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey);
    let userSbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey);
    console.log(`userZbtcAccount:${userZbtcAccount} userSbtcAccount:${userSbtcAccount}`);

    // Fund user with zBTC
    await mintTo(connection, admin.payer, zbtcMint, userZbtcAccount, admin.publicKey, 10_000_000_000);

    // Fund treasury with zBTC
    await mintTo(connection, admin.payer, zbtcMint, treasuryZbtcVault, admin.publicKey, 10_000_000_000);

    // Update mock Pyth price
    await mockPyth.methods
    .setPrice(UPDATED_PRICE, UPDATED_CONF)
    .accounts({
      price: pythPriceFeed,
      authority: admin.publicKey,
    } as any)
    .rpc();

  console.log("✅ Mock Pyth price updated to $140,000");

    const zbtcAmount = new anchor.BN(500_000_000); // 1 zBTC (8 decimals)
    const fee = zbtcAmount.toNumber() * FEE_RATE_BPS / 10_000; // 5% = 0.05 zBTC
    const netDeposit = zbtcAmount.toNumber() - fee;

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

    const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;

    console.log(`userSbtcAmount:${postUserSbtc}`);

    const sbtcAmount = new anchor.BN(Number(postUserSbtc) / 4); // 0.5 sBTC
    console.log(`sBTC amount to burn: ${sbtcAmount.toNumber()}`);
    const burnTx = await otcProgram.methods
    .burnSbtc(sbtcAmount)
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
  });
});
