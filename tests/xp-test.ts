// import { expect } from 'chai';
// import { SbtcOracle } from '../target/types/sbtc_oracle';
// import * as anchor from "@coral-xyz/anchor";
// import { BN, Program } from "@coral-xyz/anchor";
// import { OtcSwap } from "../target/types/otc_swap";
// import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
// import {
//   createMint,
//   getAccount,
//   mintTo,
//   createAssociatedTokenAccount,
//   TOKEN_PROGRAM_ID,
//   getOrCreateAssociatedTokenAccount,
// } from "@solana/spl-token";


// describe("otc-swap-local", () => {
//   const provider = anchor.AnchorProvider.local();
//   anchor.setProvider(provider);

//   const otcProgram = anchor.workspace.OtcSwap as Program<OtcSwap>;
//   const oracleProgram = anchor.workspace.SbtcOracle as Program<SbtcOracle>;
//   const connection = provider.connection;
//   const admin = provider.wallet;

//   // Test constants
//   const ZBTC_DECIMALS = 8;
//   const SBTC_DECIMALS = 8;
//   const FEE_RATE_BPS = 500; // 5%
//   const MIN_COLLATERAL_BPS = 20000; // 200%

//   // Mock prices (in cents)
//   const ZBTC_PRICE = new BN(12_500_000); // $125,000
//   const SBTC_PRICE = new BN(10_000_000); // $100,000

//   // Accounts
//   let sbtcMint: anchor.web3.PublicKey;
//   let zbtcMint: anchor.web3.PublicKey;

//   let sbtcMintAuthorityPda: anchor.web3.PublicKey;
//   let treasuryAuthorityPda: anchor.web3.PublicKey;
//   let feeAuthorityPda: anchor.web3.PublicKey;
//   let configPda: anchor.web3.PublicKey;

//   let treasuryZbtcVault: anchor.web3.PublicKey;
//   let feeVault: anchor.web3.PublicKey;

//   before(async () => {
//     // Derive PDAs
//     [configPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("config_v1"), admin.publicKey.toBuffer()],
//       otcProgram.programId
//     );

//     [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("sbtc_mint_authority"), admin.publicKey.toBuffer()],
//       otcProgram.programId
//     );

//     [treasuryAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("treasury_auth_v1"), admin.publicKey.toBuffer()],
//       otcProgram.programId
//     );

//     [feeAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("fee_auth_v1"), admin.publicKey.toBuffer()],
//       otcProgram.programId
//     );

//     // [oracleStatePda] = PublicKey.findProgramAddressSync(
//     //   [Buffer.from("oracle")],
//     //   oracleProgram.programId
//     // );
  
//     console.log(`configPda:${configPda} sbtcMintAuthorityPda:${sbtcMintAuthorityPda} treasuryAuthorityPda:${treasuryAuthorityPda} feeAuthorityPda:${feeAuthorityPda}`);

//     // Create mints
//     zbtcMint = await createMint(connection, admin.payer, admin.publicKey, null, ZBTC_DECIMALS);
//     sbtcMint = await createMint(connection, admin.payer, admin.publicKey, admin.publicKey, SBTC_DECIMALS);

//     // === Create token accounts ===
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
//   });

//   it("initialize", async () => {
//     console.log("AYO INITIALIZE");

//     const tx = await otcProgram.methods
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
//   });

//   it("mint-test", async () => {
//     console.log("AYO MINT");

//     // Create user
//     let user = Keypair.generate();
//     await connection.requestAirdrop(user.publicKey, 1e9);

//     // Create token accounts
//     let userZbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey);
//     let userSbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey);

//     // Fund user with zBTC
//     await mintTo(connection, admin.payer, zbtcMint, userZbtcAccount, admin.publicKey, 10_000_000_000);

//     // Fund treasury with zBTC
//     await mintTo(connection, admin.payer, zbtcMint, treasuryZbtcVault, admin.publicKey, 10_000_000_000);

//     const zbtcAmount = new anchor.BN(100_000_000); // 1 zBTC (8 decimals)
//     const fee = zbtcAmount.toNumber() * FEE_RATE_BPS / 10_000; // 5% = 0.05 zBTC
//     const netDeposit = zbtcAmount.toNumber() - fee;

//     // const initTx = await otcProgram.methods
//     //   .initialize(
//     //     new anchor.BN(FEE_RATE_BPS),
//     //     new anchor.BN(MIN_COLLATERAL_BPS)
//     //   )
//     //   .accounts({
//     //     squadMultisig: admin.publicKey,
//     //     sbtcMint: sbtcMint,
//     //     zbtcMint: zbtcMint,
//     //     sbtcMintAuthorityPda: sbtcMintAuthorityPda,
//     //     treasuryAuthorityPda: treasuryAuthorityPda,
//     //     feeAuthorityPda: feeAuthorityPda,
//     //     treasuryZbtcVault: treasuryZbtcVault,
//     //     feeVault: feeVault,
//     //     config: configPda,
//     //     tokenProgram: TOKEN_PROGRAM_ID,
//     //     systemProgram: SystemProgram.programId,
//     //   } as any)
//     //   .rpc();

//     // console.log("Initialize tx:", initTx);

//     // Pre-balances
//     const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
//     const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;

//     const tx = await otcProgram.methods
//       .mintSbtcTest(zbtcAmount, ZBTC_PRICE, SBTC_PRICE)
//       .accounts({
//         user: user.publicKey,
//         squadMultisig: admin.publicKey,
//         userSbtcAccount: userSbtcAccount,
//         userZbtcAccount: userZbtcAccount,
//         config: configPda,
//         sbtcMint: sbtcMint,
//         zbtcMint: zbtcMint,
//         treasuryZbtcVault: treasuryZbtcVault,
//         feeVault: feeVault,
//         sbtcMintAuthorityPda: sbtcMintAuthorityPda,
//         tokenProgram: TOKEN_PROGRAM_ID,
//       } as any)
//       .signers([user])
//       .rpc();

//     // Verify results
//     const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
//     const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
//     const config = await otcProgram.account.config.fetch(configPda);

//     // User zBTC should decrease by deposited amount
//     expect(Number(postUserZbtc)).to.be.lessThan(Number(preUserZbtc));
    
//     // User should receive sBTC
//     expect(Number(postUserSbtc)).to.be.greaterThan(Number(preUserSbtc));
    
//     // Total sBTC outstanding should increase
//     expect(Number(config.totalSbtcOutstanding)).to.be.greaterThan(0);
//   });

//   it("burn-test", async () => {
//     console.log("AYO BURN");

//     // Create user
//     let user = Keypair.generate();
//     await connection.requestAirdrop(user.publicKey, 1e9);

//     // Create token accounts
//     let userZbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, zbtcMint, user.publicKey);
//     let userSbtcAccount = await createAssociatedTokenAccount(connection, admin.payer, sbtcMint, user.publicKey);

//     const sbtcAmount = new BN(25_000_000);

//     // Fund user with zBTC
//     await mintTo(connection, admin.payer, zbtcMint, userZbtcAccount, admin.publicKey, 10_000_000_000);

//     const zbtcAmount = new anchor.BN(100_000_000); // 1 zBTC (8 decimals)
//     const fee = zbtcAmount.toNumber() * FEE_RATE_BPS / 10_000; // 5% = 0.05 zBTC
//     const netDeposit = zbtcAmount.toNumber() - fee;

//     await otcProgram.methods
//       .mintSbtcTest(zbtcAmount, ZBTC_PRICE, SBTC_PRICE)
//       .accounts({
//         user: user.publicKey,
//         squadMultisig: admin.publicKey,
//         userSbtcAccount: userSbtcAccount,
//         userZbtcAccount: userZbtcAccount,
//         config: configPda,
//         sbtcMint: sbtcMint,
//         zbtcMint: zbtcMint,
//         treasuryZbtcVault: treasuryZbtcVault,
//         feeVault: feeVault,
//         sbtcMintAuthorityPda: sbtcMintAuthorityPda,
//         tokenProgram: TOKEN_PROGRAM_ID,
//       } as any)
//       .signers([user])
//       .rpc();

//     // Pre-balances
//     const preUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
//     const preUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
//     const preConfig = await otcProgram.account.config.fetch(configPda);

//     const tx = await otcProgram.methods
//       .burnSbtcTest(sbtcAmount, ZBTC_PRICE, SBTC_PRICE)
//       .accounts({
//         user: user.publicKey,
//         squadMultisig: admin.publicKey,
//         userSbtcAccount: userSbtcAccount,
//         userZbtcAccount: userZbtcAccount,
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
//       .signers([user])
//       .rpc();

//     // Verify results
//     const postUserZbtc = (await getAccount(connection, userZbtcAccount)).amount;
//     const postUserSbtc = (await getAccount(connection, userSbtcAccount)).amount;
//     const postConfig = await otcProgram.account.config.fetch(configPda);

//     // User zBTC should decrease by deposited amount
//     expect(Number(postUserZbtc)).to.be.greaterThan(Number(preUserZbtc));
    
//     // User should receive sBTC
//     expect(Number(postUserSbtc)).to.be.lessThan(Number(preUserSbtc));
    
//     // Total sBTC outstanding should increase
//     expect(Number(postConfig.totalSbtcOutstanding)).to.be.greaterThan(0);
//   });
// });
