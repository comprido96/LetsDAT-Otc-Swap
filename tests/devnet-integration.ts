import { expect } from 'chai';
import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { OtcSwap } from "../target/types/otc_swap";
import { SbtcOracle } from '../target/types/sbtc_oracle';
import { MockPyth } from "../target/types/mock_pyth";
import { PublicKey, Keypair, SystemProgram, SendTransactionError } from "@solana/web3.js";
import {
  createMint,
  getAccount,
  mintTo,
  createAssociatedTokenAccount,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  getOrCreateAssociatedTokenAccount,
  getMint,
} from "@solana/spl-token";
import fs from 'fs';
import bs58 from 'bs58';


describe("devnet-integration", () => {
  function loadKeypair(privateKey: string): Keypair {
    // try to load privateKey as a filepath
    let loadedKey: Uint8Array;
    if (fs.existsSync(privateKey)) {
      privateKey = fs.readFileSync(privateKey).toString();
    }

    if (privateKey.includes('[') && privateKey.includes(']')) {
      loadedKey = Uint8Array.from(JSON.parse(privateKey));
    } else if (privateKey.includes(',')) {
      loadedKey = Uint8Array.from(
        privateKey.split(',').map((val) => Number(val))
      );
    } else {
      privateKey = privateKey.replace(/\s/g, '');
      loadedKey = new Uint8Array(bs58.decode(privateKey));
    }

    return Keypair.fromSecretKey(Uint8Array.from(loadedKey));
  }

  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const otcProgram = anchor.workspace.OtcSwap as Program<OtcSwap>;
  const oracleProgram = anchor.workspace.SbtcOracle as Program<SbtcOracle>;
  const mockPyth = anchor.workspace.MockPyth as Program<MockPyth>;

  const connection = provider.connection;
  const admin = provider.wallet;

  const keyPairFile = `${process.env.USER_WALLET}`;
  const user = loadKeypair(keyPairFile);

  // === Test constants ===
  const ZBTC_DECIMALS = 8;
  const SBTC_DECIMALS = 8;
  const FEE_RATE_BPS = 500; // 5%
  const MIN_COLLATERAL_BPS = 20000; // 200%

  const NEW_ORACLE_TREND = new BN(4555883.7282848894);

  // === Accounts ===
  let sbtcMint: anchor.web3.PublicKey = new PublicKey("7dMm9RgrkknPkrp7n1sgkbJFPkG5pAZzEs32NcyjeDkW");
  let zbtcMint: anchor.web3.PublicKey = new PublicKey("91AgzqSfXnCq6AJm5CPPHL3paB25difEJ1TfSnrFKrf");

  let sbtcMintAuthorityPda: anchor.web3.PublicKey;
  let treasuryAuthorityPda: anchor.web3.PublicKey;
  let feeAuthorityPda: anchor.web3.PublicKey;
  let configPda: anchor.web3.PublicKey;

  let treasuryZbtcVault: anchor.web3.PublicKey;
  let feeVault: anchor.web3.PublicKey;

  let oracleStatePda: anchor.web3.PublicKey;
  let pythPriceFeed: anchor.web3.PublicKey = new PublicKey("HovQMDrbAgAYPCmHVSrezcSmkMtXSSUsLDFANExrZh2J");

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
    console.log(`oracleStatePda:${oracleStatePda}`);

    // === Create token accounts ===
    const treasuryAccount = await getOrCreateAssociatedTokenAccount(
        connection,
        admin.payer,
        zbtcMint,
        treasuryAuthorityPda, // PDA as owner
        true, // allowOwnerOffCurve - for PDAs
        undefined, // confirmOptions
        undefined, // programId (uses TOKEN_PROGRAM_ID)
        TOKEN_PROGRAM_ID // explicitly specify token otcProgram
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

  // // this will only pass the first time you initialize otcProgram
  // it("initialize", async () => {
  //   console.log("AYO INITIALIZE");

  //   // === Verify sBTC initially owned by admin (squad) ===
  //   let sbtcMintInfo = await getMint(connection, sbtcMint, null, TOKEN_PROGRAM_ID);
  //   console.log(sbtcMintInfo);

  //   expect(sbtcMintInfo.mintAuthority?.equals(admin.publicKey)).to.be.true;

  //   console.log("Initializing otcSwap...");
  //   const tx = await otcProgram.methods
  //     .initialize(
  //       new anchor.BN(FEE_RATE_BPS),
  //       new anchor.BN(MIN_COLLATERAL_BPS),
  //       pythPriceFeed,
  //       oracleStatePda,
  //     )
  //     .accounts({
  //       squadMultisig: admin.publicKey,
  //       sbtcMint: sbtcMint,
  //       zbtcMint: zbtcMint,
  //       sbtcMintAuthorityPda: sbtcMintAuthorityPda,
  //       treasuryAuthorityPda: treasuryAuthorityPda,
  //       feeAuthorityPda: feeAuthorityPda,
  //       treasuryZbtcVault: treasuryZbtcVault,
  //       feeVault: feeVault,
  //       config: configPda,
  //       tokenProgram: TOKEN_PROGRAM_ID,
  //       systemProgram: SystemProgram.programId,
  //     } as any)
  //     .rpc();

  //   console.log("Initialize tx:", tx);

  //   // === Verify sBTC authority was transferred to program PDA ===
  //   sbtcMintInfo = await getMint(connection, sbtcMint);
  //   // expect(sbtcMintInfo.mintAuthority?.equals(sbtcMintAuthorityPda)).to.be.true;

  //   // === Verify vaults are correct ===
  //   const treasuryAccount = await getAccount(connection, treasuryZbtcVault);
  //   const feeAccount = await getAccount(connection, feeVault);
  //   expect(treasuryAccount.owner.equals(treasuryAuthorityPda)).to.be.true;
  //   expect(feeAccount.owner.equals(feeAuthorityPda)).to.be.true;
  //   expect(treasuryAccount.mint.equals(zbtcMint)).to.be.true;
  //   expect(feeAccount.mint.equals(zbtcMint)).to.be.true;

  //   // === Verify config was stored ===
  //   const config = await otcProgram.account.config.fetch(configPda);
  //   expect(config.squadMultisig.equals(admin.publicKey)).to.be.true;
  //   expect(config.sbtcMint.equals(sbtcMint)).to.be.true;
  //   expect(config.zbtcMint.equals(zbtcMint)).to.be.true;
  //   expect(config.treasuryZbtcVault.equals(treasuryZbtcVault)).to.be.true;
  //   expect(config.feeVault.equals(feeVault)).to.be.true;
  //   expect(config.feeRateBps.toNumber()).to.equal(FEE_RATE_BPS);
  //   expect(config.minCollateralBps.toNumber()).to.equal(MIN_COLLATERAL_BPS);
  //   expect(config.paused).to.be.false;
  //   expect(config.totalSbtcOutstanding.toString()).to.equal("0");
  //   expect(config.authorizedZbtcPythFeed.equals(pythPriceFeed)).to.be.true;
  //   expect(config.authorizedSbtcOracleStatePda.equals(oracleStatePda)).to.be.true;
  // });

  // // The treasury must be overcollateralized already (?) But anyway you can try without doing it to see whether the mint instruction succeeds
  // it("mint", async () => {
  //   console.log("AYO MINT");

  //   // await otcProgram.methods
  //   //   .updatePythFeed(
  //   //     pythPriceFeed,
  //   //   )
  //   //   .accounts({
  //   //     config: configPda,
  //   //     squadMultisig: admin.publicKey,
  //   //   } as any)
  //   //   .rpc();
  //   // console.log("PYTH UPDATED!!!");

  //   console.log(`user:${user.publicKey.toBase58()}`);

  //   // === Create token accounts ===
  //   let userZbtcAccount = (await getOrCreateAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey)).address;
  //   let userSbtcAccount = (await getOrCreateAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey)).address;
  //   console.log(`userZbtcAccount:${userZbtcAccount} userSbtcAccount:${userSbtcAccount}`);

  //   const deposit = new anchor.BN(100_000); // 0.001 (8 decimals)
  //   const fee = deposit.toNumber() * FEE_RATE_BPS / 10_000;
  //   const netDeposit = deposit.toNumber() - fee;
  //   console.log(`Deposit:${deposit} net:${netDeposit}`);

  //   // Pre balances
  //   const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
  //   const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
  //   const preTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
  //   const preFee = (await getAccount(connection, feeVault)).amount;
  //   const preConfig = await otcProgram.account.config.fetch(configPda);

  //   console.log("Pre-mint balances:");
  //   console.log("User zBTC:", preUserZbtc.toString());
  //   console.log("User sBTC:", preUserSbtc.toString());
  //   console.log("Treasury:", preTreasury.toString());
  //   console.log("Fee vault:", preFee.toString());
  //   console.log("Total sBTC outstanding:", preConfig.totalSbtcOutstanding.toString());

  //   try {
  //     const tx = await otcProgram.methods
  //     .mintSbtc(deposit)
  //     .accounts({
  //       user: user.publicKey,
  //       squadMultisig: admin.publicKey,
  //       config: configPda,
  //       sbtcMint: sbtcMint,
  //       zbtcMint: zbtcMint,
  //       userSbtcAccount: userSbtcAccount,
  //       userZbtcAccount: userZbtcAccount,
  //       treasuryZbtcVault: treasuryZbtcVault,
  //       feeVault: feeVault,
  //       sbtcMintAuthorityPda: sbtcMintAuthorityPda,
  //       treasuryAuthorityPda: treasuryAuthorityPda,
  //       feeAuthorityPda: feeAuthorityPda,
  //       pythPriceAccount: pythPriceFeed,
  //       oracleState: oracleStatePda,
  //       tokenProgram: TOKEN_PROGRAM_ID,
  //     } as any)
  //     .signers([user])
  //     .rpc();

  //     console.log("✅ sBTC minted successfully, tx:", tx);

  //     // === Post balances ===
  //     const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
  //     const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
  //     const postTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
  //     const postFee = (await getAccount(connection, feeVault)).amount;
  //     const postConfig = await otcProgram.account.config.fetch(configPda);

  //     console.log("Post-mint balances:");
  //     console.log("User zBTC:", postUserZbtc.toString());
  //     console.log("User sBTC:", postUserSbtc.toString());
  //     console.log("Treasury:", postTreasury.toString());
  //     console.log("Fee vault:", postFee.toString());
  //     console.log("Total sBTC outstanding:", postConfig.totalSbtcOutstanding.toString());

  //     // === Assertions ===
  //     expect(postUserZbtc.toString()).to.equal((Number(preUserZbtc) - deposit.toNumber()).toString());
  //     expect(postTreasury.toString()).to.equal((Number(preTreasury) + netDeposit).toString());
  //     expect(postFee.toString()).to.equal((Number(preFee) + fee).toString());
  //     expect(postUserSbtc > preUserSbtc).to.be.true;

  //     // === Config check ===
  //     const config = await otcProgram.account.config.fetch(configPda);
  //     expect(config.totalSbtcOutstanding.toString()).to.equal(postUserSbtc.toString());
  //   } catch(e) {
  //     await e.getLogs()
  //     console.log(await e.getLogs());
  //   }
  // });

  // it("burn", async () => {
  //   console.log("AYO BURN");

  //   // await otcProgram.methods
  //   //   .updatePythFeed(
  //   //     pythPriceFeed,
  //   //   )
  //   //   .accounts({
  //   //     config: configPda,
  //   //     squadMultisig: admin.publicKey,
  //   //   } as any)
  //   //   .rpc();
  //   // console.log("PYTH UPDATED!!!");

  //   console.log(`user:${user.publicKey.toBase58()}`);

  //   // === Create token accounts ===
  //   let userZbtcAccount = (await getOrCreateAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey)).address;
  //   let userSbtcAccount = (await getOrCreateAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey)).address;
  //   console.log(`userZbtcAccount:${userZbtcAccount} userSbtcAccount:${userSbtcAccount}`);

  //   const burnAmount = new anchor.BN(113_151);

  //   // === Pre balances ===
  //   const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
  //   const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
  //   const preTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
  //   const preFee = (await getAccount(connection, feeVault)).amount;
  //   const preConfig = await otcProgram.account.config.fetch(configPda);

  //   console.log("Pre-burn balances:");
  //   console.log("User zBTC:", preUserZbtc.toString());
  //   console.log("User sBTC:", preUserSbtc.toString());
  //   console.log("Treasury:", preTreasury.toString());
  //   console.log("Fee vault:", preFee.toString());
  //   console.log("Total sBTC outstanding:", preConfig.totalSbtcOutstanding.toString());

  //   try{ 
  //     const burnTx = await otcProgram.methods
  //     .burnSbtc(burnAmount)
  //     .accounts({
  //       user: user.publicKey,
  //       squadMultisig: admin.publicKey,
  //       config: configPda,
  //       sbtcMint: sbtcMint,
  //       zbtcMint: zbtcMint,
  //       userSbtcAccount: userSbtcAccount,
  //       userZbtcAccount: userZbtcAccount,
  //       treasuryZbtcVault: treasuryZbtcVault,
  //       feeVault: feeVault,
  //       treasuryAuthorityPda: treasuryAuthorityPda,
  //       feeAuthorityPda: feeAuthorityPda,
  //       pythPriceAccount: pythPriceFeed,
  //       oracleState: oracleStatePda,
  //       tokenProgram: TOKEN_PROGRAM_ID,
  //     } as any)
  //     .signers([user])
  //     .rpc();

  //     console.log("✅ sBTC burned successfully, tx:", burnTx);

  //     // === Post balances ===
  //     const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
  //     const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
  //     const postTreasury = (await getAccount(connection, treasuryZbtcVault)).amount;
  //     const postFee = (await getAccount(connection, feeVault)).amount;
  //     const postConfig = await otcProgram.account.config.fetch(configPda);

  //     console.log("Post-burn balances:");
  //     console.log("User zBTC:", postUserZbtc.toString());
  //     console.log("User sBTC:", postUserSbtc.toString());
  //     console.log("Treasury:", postTreasury.toString());
  //     console.log("Fee vault:", postFee.toString());
  //     console.log("Total sBTC outstanding:", postConfig.totalSbtcOutstanding.toString());

  //     // === Assertions ===
  //     // User sBTC should be burned
  //     expect(Number(postUserSbtc)).to.equal(Number(preUserSbtc) - burnAmount.toNumber());

  //     // Total sBTC outstanding should decrease
  //     expect(postConfig.totalSbtcOutstanding.toString()).to.equal(
  //       (Number(preConfig.totalSbtcOutstanding) - burnAmount.toNumber()).toString()
  //     );
  //   }
  //   catch(e) {
  //     console.error("Error while burning.");
  //     console.error(e);
  //     throw e;
  //   }
  // });

  it("test oracle reading", async () => {
    try {
      const tx = await otcProgram.methods
        .testOracleReading()
        .accounts({
          oracleState: oracleStatePda,
        })
        .rpc();
      
      console.log("Oracle reading test SUCCESS - Transaction:", tx);
    } catch (error) {
      console.log("Oracle reading test FAILED");
      console.log("Error:", error.message);
      
      // Check if oracle account exists and has data
      try {
        const oracleAccountInfo = await connection.getAccountInfo(oracleStatePda);
        if (oracleAccountInfo) {
          console.log("Oracle account exists, data length:", oracleAccountInfo.data.length);
          console.log("First 32 bytes:", Buffer.from(oracleAccountInfo.data.slice(0, 32)).toString('hex'));
        } else {
          console.log("Oracle account does not exist!");
        }
      } catch (e) {
        console.log("Could not fetch oracle account info:", e.message);
      }
    }
  });

  it("test pyth reading", async () => {
    try {
      const tx = await otcProgram.methods
        .testPythReading()
        .accounts({
          pythPriceAccount: pythPriceFeed,
        })
        .rpc();
      
      console.log("Pyth reading test SUCCESS - Transaction:", tx);
    } catch (error) {
      console.log("Pyth reading test FAILED");
      console.log("Error:", error.message);
      
      // Debug the Pyth account
      try {
        const pythAccountInfo = await connection.getAccountInfo(pythPriceFeed);
        if (pythAccountInfo) {
          console.log("Pyth account exists, data length:", pythAccountInfo.data.length);
        } else {
          console.log("Pyth account does not exist!");
        }
      } catch (e) {
        console.log("Could not fetch Pyth account info:", e.message);
      }
    }
  });
});
