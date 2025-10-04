import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { OtcSwap } from "../target/types/otc_swap";
import chai from 'chai';
import chaiAsPromised from 'chai-as-promised';
import { ParsedAccountData, PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  createMint,
  createAccount,
  getAccount,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
  mintTo,
  transfer,
  getMint,
  createAssociatedTokenAccount,
  getOrCreateAssociatedTokenAccount,
} from "@solana/spl-token";
import { SYSTEM_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/native/system";


chai.use(chaiAsPromised);
const expect = chai.expect;


// describe("otc-swap: initialize", () => {
//   const provider = anchor.AnchorProvider.local();
//   anchor.setProvider(provider);

//   const program = anchor.workspace.OtcSwap as Program<OtcSwap>;
//   const admin = provider.wallet;
//   const connection = provider.connection;


//   // Constants
//   const FEE_RATE_BPS = 50; // 0.5%
//   const MIN_COLLATERAL_BPS = 25000; // 250%


//   let sbtcMint: PublicKey;
//   let zbtcMint: PublicKey;
//   let sbtcMintAuthorityPda: PublicKey;
//   let treasuryAuthorityPda: PublicKey;
//   let feeAuthorityPda: PublicKey;
//   let treasuryZbtcVault: PublicKey;
//   let feeVault: PublicKey;
//   let configPda: PublicKey;

//   // Helper PDA derivations
//   const derivePdas = (squadWallet: PublicKey) => {
//     // === Derive PDAs ===
//     [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("sbtc_mint_authority"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     [treasuryAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("treasury_auth_v1"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     [feeAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("fee_auth_v1"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     [configPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("config_v1"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     return { sbtcMintAuthorityPda, treasuryAuthorityPda, feeAuthorityPda, configPda };
//   };

// before(async () => {
//     // === Create mints ===
//     zbtcMint = await createMint(connection, admin.payer, admin.publicKey, null, 8);
//     sbtcMint = await createMint(connection, admin.payer, admin.publicKey, admin.publicKey, 8);

//     const { sbtcMintAuthorityPda, treasuryAuthorityPda, feeAuthorityPda, configPda } = derivePdas(admin.publicKey);

//     // === Create token accounts BEFORE initialize ===
//     const treasuryAccount = await getOrCreateAssociatedTokenAccount(
//         connection,
//         admin.payer,
//         zbtcMint,
//         treasuryAuthorityPda, // PDA as owner
//         true, // allowOwnerOffCurve - IMPORTANT for PDAs
//         undefined, // confirmOptions
//         undefined, // programId (uses TOKEN_PROGRAM_ID)
//         TOKEN_PROGRAM_ID // explicitly specify token program
//     );
//     treasuryZbtcVault = treasuryAccount.address;

//     const feeAccount = await getOrCreateAssociatedTokenAccount(
//         connection,
//         admin.payer,
//         zbtcMint,
//         feeAuthorityPda, // PDA as owner
//         true, // allowOwnerOffCurve - IMPORTANT for PDAs
//         undefined,
//         undefined,
//         TOKEN_PROGRAM_ID
//     );
//     feeVault = feeAccount.address;
// });
//   it("Should initialize and transfer sBTC mint authority", async () => {
//     console.log("AYO");

//     // Verify sBTC initially owned by admin (squad)
//     let mintInfo = await getMint(connection, sbtcMint);
//     expect(mintInfo.mintAuthority?.equals(admin.publicKey)).to.be.true;

//     const tx = await program.methods
//       .initialize(
//         new anchor.BN(FEE_RATE_BPS),
//         new anchor.BN(MIN_COLLATERAL_BPS)
//       )
//       .accounts({
//         squadMultisig: admin.publicKey,
//         sbtcMint: sbtcMint,
//         zbtcMint: zbtcMint,
//         sbtcMintAuthorityPda: sbtcMintAuthorityPda,
//         treasuryAuthorityPda: treasuryAuthorityPda,
//         feeAuthorityPda: feeAuthorityPda,
//         treasuryZbtcVault: treasuryZbtcVault,
//         feeVault: feeVault,
//         config: configPda,
//         tokenProgram: TOKEN_PROGRAM_ID,
//         systemProgram: SystemProgram.programId,
//       } as any)
//       .rpc();

//     console.log("Initialize tx:", tx);

//     // Verify sBTC authority was transferred to program PDA
//     mintInfo = await getMint(connection, sbtcMint);
//     expect(mintInfo.mintAuthority?.equals(sbtcMintAuthorityPda)).to.be.true;

//     // Verify vaults are correct
//     const treasuryAccount = await getAccount(connection, treasuryZbtcVault);
//     const feeAccount = await getAccount(connection, feeVault);
//     expect(treasuryAccount.owner.equals(treasuryAuthorityPda)).to.be.true;
//     expect(feeAccount.owner.equals(feeAuthorityPda)).to.be.true;
//     expect(treasuryAccount.mint.equals(zbtcMint)).to.be.true;
//     expect(feeAccount.mint.equals(zbtcMint)).to.be.true;

//     // Verify config was stored
//     const config = await program.account.config.fetch(configPda);
//     expect(config.squadMultisig.equals(admin.publicKey)).to.be.true;
//     expect(config.sbtcMint.equals(sbtcMint)).to.be.true;
//     expect(config.zbtcMint.equals(zbtcMint)).to.be.true;
//     expect(config.treasuryZbtcVault.equals(treasuryZbtcVault)).to.be.true;
//     expect(config.feeVault.equals(feeVault)).to.be.true;
//     expect(config.feeRateBps.toNumber()).to.equal(FEE_RATE_BPS);
//     expect(config.minCollateralBps.toNumber()).to.equal(MIN_COLLATERAL_BPS);
//     expect(config.paused).to.be.false;
//     expect(config.totalSbtcOutstanding.toString()).to.equal("0");
//   });
// });

// describe("mint_sbtc", () => {
//   const provider = anchor.AnchorProvider.local();
//   anchor.setProvider(provider);

//   const program = anchor.workspace.OtcSwap as Program<OtcSwap>;
//   const connection = provider.connection;
//   const admin = provider.wallet;

//   let sbtcMint: anchor.web3.PublicKey;
//   let zbtcMint: anchor.web3.PublicKey;
//   let user: anchor.web3.Keypair;
//   let priceAccount: anchor.web3.PublicKey;

//   let userZbtcAccount: anchor.web3.PublicKey;
//   let userSbtcAccount: anchor.web3.PublicKey;
//   let treasuryZbtcVault: anchor.web3.PublicKey;
//   let feeVault: anchor.web3.PublicKey;
//   let sbtcMintAuthorityPda: anchor.web3.PublicKey;
//   let treasuryAuthorityPda: anchor.web3.PublicKey;
//   let feeAuthorityPda: anchor.web3.PublicKey;
//   let configPda: anchor.web3.PublicKey;

//   const FEE_RATE_BPS = 500; // 5%
//   const MIN_COLLATERAL_BPS = 20000; // 200%

//   // Helper PDA derivations
//   const derivePdas = (squadWallet: PublicKey) => {
//     // === Derive PDAs ===
//     [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("sbtc_mint_authority"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     [treasuryAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("treasury_auth_v1"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     [feeAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("fee_auth_v1"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     [configPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("config_v1"), admin.publicKey.toBuffer()],
//       program.programId
//     );

//     return { sbtcMintAuthorityPda, treasuryAuthorityPda, feeAuthorityPda, configPda };
//   };

//   before(async () => {
//     // === Create mints ===
//     zbtcMint = await createMint(connection, admin.payer, admin.publicKey, null, 8);
//     sbtcMint = await createMint(connection, admin.payer, admin.publicKey, admin.publicKey, 8);

//     const { sbtcMintAuthorityPda, treasuryAuthorityPda, feeAuthorityPda, configPda } = derivePdas(admin.publicKey);

//     // === Create token accounts BEFORE initialize ===
//     const treasuryAccount = await getOrCreateAssociatedTokenAccount(
//         connection,
//         admin.payer,
//         zbtcMint,
//         treasuryAuthorityPda, // PDA as owner
//         true, // allowOwnerOffCurve - IMPORTANT for PDAs
//         undefined, // confirmOptions
//         undefined, // programId (uses TOKEN_PROGRAM_ID)
//         TOKEN_PROGRAM_ID // explicitly specify token program
//     );
//     treasuryZbtcVault = treasuryAccount.address;

//     const feeAccount = await getOrCreateAssociatedTokenAccount(
//         connection,
//         admin.payer,
//         zbtcMint,
//         feeAuthorityPda, // PDA as owner
//         true, // allowOwnerOffCurve - IMPORTANT for PDAs
//         undefined,
//         undefined,
//         TOKEN_PROGRAM_ID
//     );
//     feeVault = feeAccount.address;

//     // === Create user + accounts ===
//     user = anchor.web3.Keypair.generate();
//     await provider.connection.requestAirdrop(user.publicKey, 2e9); // 2 SOL for tx fees

//     priceAccount = anchor.web3.Keypair.generate().publicKey;

//     userZbtcAccount = await createAccount(connection, admin.payer, zbtcMint, user.publicKey);
//     userSbtcAccount = await createAccount(connection, admin.payer, sbtcMint, user.publicKey);

//     // Fund user with zBTC
//     await mintTo(
//       connection,
//       admin.payer,
//       zbtcMint,
//       userZbtcAccount,
//       admin.publicKey,
//       1_000_000_000 // 10 zBTC (8 decimals)
//     );
//   });

//   it("mints sBTC when depositing zBTC", async () => {
//     console.log("AYO MINT");
//     const deposit = new anchor.BN(100_000_000); // 1 zBTC (8 decimals)
//     const fee = deposit.toNumber() * FEE_RATE_BPS / 10_000; // 5% = 0.05 zBTC
//     const netDeposit = deposit.toNumber() - fee;

//     const tx = await program.methods
//       .initialize(
//         new anchor.BN(FEE_RATE_BPS),
//         new anchor.BN(MIN_COLLATERAL_BPS)
//       )
//       .accounts({
//         squadMultisig: admin.publicKey,
//         sbtcMint: sbtcMint,
//         zbtcMint: zbtcMint,
//         sbtcMintAuthorityPda: sbtcMintAuthorityPda,
//         treasuryAuthorityPda: treasuryAuthorityPda,
//         feeAuthorityPda: feeAuthorityPda,
//         treasuryZbtcVault: treasuryZbtcVault,
//         feeVault: feeVault,
//         config: configPda,
//         tokenProgram: TOKEN_PROGRAM_ID,
//         systemProgram: SystemProgram.programId,
//       } as any)
//       .rpc();

//     console.log("Initialize tx:", tx);

//     // Pre balances
//     const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
//     const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
//     const preTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
//     const preFee = (await getAccount(connection, feeVault)).amount;

//     // === Call mint_sbtc ===
//     await program.methods
//     .mintSbtc(deposit)
//     .accounts({
//       user: user.publicKey,
//       squadMultisig: admin.publicKey,
//       priceAccount: priceAccount,
//       userSbtcAccount: userSbtcAccount,
//       userZbtcAccount: userZbtcAccount,
//       config: configPda,
//       sbtcMint: sbtcMint,
//       zbtcMint: zbtcMint,
//       treasuryZbtcVault: treasuryZbtcVault,
//       feeVault: feeVault,
//       sbtcMintAuthorityPda: sbtcMintAuthorityPda,
//       tokenProgram: TOKEN_PROGRAM_ID,
//     } as any)
//     .signers([user])
//     .rpc();

//     // Post balances
//     const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
//     const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
//     const postTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
//     const postFee = (await getAccount(connection, feeVault)).amount;

//     // === Assertions ===
//     expect(postUserZbtc.toString()).to.equal((Number(preUserZbtc) - deposit.toNumber()).toString());
//     expect(postTreasury.toString()).to.equal((Number(preTreasury) + netDeposit).toString());
//     expect(postFee.toString()).to.equal((Number(preFee) + fee).toString());
//     expect(postUserSbtc > preUserSbtc).to.be.true;

//     // Config check
//     const config = await program.account.config.fetch(configPda);
//     expect(config.totalSbtcOutstanding.toString()).to.equal(postUserSbtc.toString());
//   });
// });

describe("burn_sbtc", () => {
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);

  const program = anchor.workspace.OtcSwap as Program<OtcSwap>;
  const connection = provider.connection;
  const admin = provider.wallet;

  let sbtcMint: anchor.web3.PublicKey;
  let zbtcMint: anchor.web3.PublicKey;
  let user: anchor.web3.Keypair;
  let priceAccount: anchor.web3.PublicKey;

  let userZbtcAccount: anchor.web3.PublicKey;
  let userSbtcAccount: anchor.web3.PublicKey;
  let treasuryZbtcVault: anchor.web3.PublicKey;
  let feeVault: anchor.web3.PublicKey;
  let sbtcMintAuthorityPda: anchor.web3.PublicKey;
  let treasuryAuthorityPda: anchor.web3.PublicKey;
  let feeAuthorityPda: anchor.web3.PublicKey;
  let configPda: anchor.web3.PublicKey;

  const FEE_RATE_BPS = 500; // 5%
  const MIN_COLLATERAL_BPS = 20000; // 200%

  // Helper PDA derivations
  const derivePdas = (squadWallet: PublicKey) => {
    [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("sbtc_mint_authority"), squadWallet.toBuffer()],
      program.programId
    );

    [treasuryAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("treasury_auth_v1"), squadWallet.toBuffer()],
      program.programId
    );

    [feeAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fee_auth_v1"), squadWallet.toBuffer()],
      program.programId
    );

    [configPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("config_v1"), squadWallet.toBuffer()],
      program.programId
    );

    return { sbtcMintAuthorityPda, treasuryAuthorityPda, feeAuthorityPda, configPda };
  };

  before(async () => {
    // === Create mints ===
    zbtcMint = await createMint(connection, admin.payer, admin.publicKey, null, 8);
    sbtcMint = await createMint(connection, admin.payer, admin.publicKey, admin.publicKey, 8);

    const { sbtcMintAuthorityPda, treasuryAuthorityPda, feeAuthorityPda, configPda } = derivePdas(admin.publicKey);

    // === Create token accounts BEFORE initialize ===
    const treasuryAccount = await getOrCreateAssociatedTokenAccount(
      connection,
      admin.payer,
      zbtcMint,
      treasuryAuthorityPda,
      true,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
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

    // === Create user + accounts ===
    user = anchor.web3.Keypair.generate();
    await provider.connection.requestAirdrop(user.publicKey, 2e9);

    priceAccount = anchor.web3.Keypair.generate().publicKey;

    userZbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey);
    userSbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey);

    // Fund user with zBTC
    await mintTo(
      connection,
      admin.payer,
      zbtcMint,
      userZbtcAccount,
      admin.publicKey,
      1_000_000_000 // 10 zBTC
    );

    // Fund treasury with zBTC for redemptions
    await mintTo(
      connection,
      admin.payer,
      zbtcMint,
      treasuryZbtcVault,
      admin.publicKey,
      2_000_000_000 // 20 zBTC
    );

    // === Initialize program ===
    await program.methods
      .initialize(new anchor.BN(FEE_RATE_BPS), new anchor.BN(MIN_COLLATERAL_BPS))
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

    // === First mint some sBTC to user so they have something to burn ===
    const mintAmount = new anchor.BN(100_000_000); // 1 sBTC
    await program.methods
      .mintSbtc(mintAmount)
      .accounts({
        user: user.publicKey,
        squadMultisig: admin.publicKey,
        priceAccount: priceAccount,
        userSbtcAccount: userSbtcAccount,
        userZbtcAccount: userZbtcAccount,
        config: configPda,
        sbtcMint: sbtcMint,
        zbtcMint: zbtcMint,
        treasuryZbtcVault: treasuryZbtcVault,
        feeVault: feeVault,
        sbtcMintAuthorityPda: sbtcMintAuthorityPda,
        tokenProgram: TOKEN_PROGRAM_ID,
      } as any)
      .signers([user])
      .rpc();

    console.log("Setup complete - user has sBTC to burn");
  });

  it("burns sBTC and redeems zBTC", async () => {
    const burnAmount = new anchor.BN(50_000_000); // 0.5 sBTC
    const fee = burnAmount.toNumber() * FEE_RATE_BPS / 10_000; // 5% fee
    const netZbtc = burnAmount.toNumber() - fee; // 1:1 price assumption

    // Pre balances
    const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
    const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
    const preTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
    const preFee = (await getAccount(connection, feeVault)).amount;
    const preConfig = await program.account.config.fetch(configPda);

    console.log("Pre-burn balances:");
    console.log("User zBTC:", preUserZbtc.toString());
    console.log("User sBTC:", preUserSbtc.toString());
    console.log("Treasury:", preTreasury.toString());
    console.log("Fee vault:", preFee.toString());
    console.log("Total sBTC outstanding:", preConfig.totalSbtcOutstanding.toString());

    // === Call burn_sbtc ===
    await program.methods
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
        tokenProgram: TOKEN_PROGRAM_ID,
      } as any)
      .signers([user])
      .rpc();

    // Post balances
    const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
    const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
    const postTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
    const postFee = (await getAccount(connection, feeVault)).amount;
    const postConfig = await program.account.config.fetch(configPda);

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
    expect(Number(postTreasury)).to.equal(Number(preTreasury) - burnAmount.toNumber());
    
    // Fee vault should increase by fee amount
    expect(Number(postFee)).to.equal(Number(preFee) + fee);
    
    // Total sBTC outstanding should decrease
    expect(postConfig.totalSbtcOutstanding.toString()).to.equal(
      (Number(preConfig.totalSbtcOutstanding) - burnAmount.toNumber()).toString()
    );
  });

  it("fails when burning zero amount", async () => {
    try {
      await program.methods
        .burnSbtc(new anchor.BN(0))
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
          tokenProgram: TOKEN_PROGRAM_ID,
        } as any)
        .signers([user])
        .rpc();
      
      expect.fail("Should have failed with InvalidAmount error");
    } catch (error) {
      expect(error.message).to.include("InvalidAmount");
    }
  });

  it("fails when user has insufficient sBTC balance", async () => {
    const userBalance = (await getAccount(connection, userSbtcAccount)).amount;
    const excessiveBurn = new anchor.BN(Number(userBalance) + 1_000_000); // More than user has

    try {
      await program.methods
        .burnSbtc(excessiveBurn)
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
          tokenProgram: TOKEN_PROGRAM_ID,
        } as any)
        .signers([user])
        .rpc();
      
      expect.fail("Should have failed with InsufficientBalance error");
    } catch (error) {
      expect(error.message).to.include("InsufficientBalance");
    }
  });

  it("fails when treasury has insufficient zBTC", async () => {
    // Try to burn a very large amount that would exceed treasury reserves
    const largeBurnAmount = new anchor.BN(1_000_000_000); // 10 sBTC

    try {
      await program.methods
        .burnSbtc(largeBurnAmount)
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
          tokenProgram: TOKEN_PROGRAM_ID,
        } as any)
        .signers([user])
        .rpc();
      
      expect.fail("Should have failed with InsufficientBalance error");
    } catch (error) {
      expect(error.message).to.include("InsufficientBalance");
    }
  });
});
