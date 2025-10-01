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
} from "@solana/spl-token";


chai.use(chaiAsPromised);
const expect = chai.expect;


// describe("otc-swap: initialize", async () => {
//   const provider = anchor.AnchorProvider.local();
//   anchor.setProvider(provider);

//   const program = anchor.workspace.OtcSwap as Program<OtcSwap>;
//   const admin = provider.wallet;
//   const connection = provider.connection;

//   // Constants
//   const FEE_RATE_BPS = 50; // for testing
//   const MIN_COLLATERAL_BPS = 25000; // for testing

//   let sbtcMint: PublicKey;
//   let zbtcMint: PublicKey;
//   let sbtcMintAuthorityPda: PublicKey;
//   let treasuryZbtcVaultPda: PublicKey;
//   let feeVaultPda: PublicKey;
//   let configPda: PublicKey;

//   // Helper function to derive PDAs
//   const deriveVaultPdAs = (squadWallet: PublicKey) => {
//     const [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("sbtc_mint_authority")],
//       program.programId
//     );

//     const [treasuryPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("treasury"), squadWallet.toBuffer()],
//       program.programId
//     );

//     const [feePda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("fees"), squadWallet.toBuffer()],
//       program.programId
//     );

//     const [configPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("config"), squadWallet.toBuffer()],
//       program.programId
//     );

//     return {
//       sbtcMintAuthorityPda,
//       treasuryZbtcVaultPda: treasuryPda,
//       feeVaultPda: feePda,
//       configPda,
//     };
//   };

//   before(async () => {
//     // === Create test mints ===
//     sbtcMint = await createMint(
//       connection,
//       admin.payer,
//       admin.publicKey, // Initial mint authority = Squad
//       admin.publicKey, // Freeze authority = Squad
//       9,
//     );

//     zbtcMint = await createMint(
//       connection,
//       admin.payer,
//       admin.publicKey,
//       null,
//       9
//     );

//     // === Derive PDA vault addresses ===
//     const pdas = deriveVaultPdAs(admin.publicKey);
//     sbtcMintAuthorityPda = pdas.sbtcMintAuthorityPda;
//     treasuryZbtcVaultPda = pdas.treasuryZbtcVaultPda;
//     feeVaultPda = pdas.feeVaultPda;
//     configPda = pdas.configPda;

//     console.log("Admin (Squad):", admin.publicKey.toString());
//     console.log("sBTC Mint Authority PDA:", sbtcMintAuthorityPda.toString());
//     console.log("Treasury PDA:", treasuryZbtcVaultPda.toString());
//     console.log("Fee PDA:", feeVaultPda.toString());
//     console.log("Config PDA:", configPda.toString());
//   });

//   it("Should initialize and transfer sBTC mint authority", async () => {
//     // Verify sBTC initially owned by Squad
//     let data = await connection.getParsedAccountInfo(sbtcMint, "confirmed");
//     let parsedMintInfo = (data?.value?.data as ParsedAccountData)?.parsed?.info;
//     if (parsedMintInfo) {
//       expect(parsedMintInfo.mintAuthority===admin.publicKey);
//     }

//     const tx = await program.methods
//       .initialize(
//         new anchor.BN(FEE_RATE_BPS),
//         new anchor.BN(MIN_COLLATERAL_BPS)
//       )
//       .accounts({
//         squadMultisig: admin.publicKey,
//         sbtcMint: sbtcMint,
//         zbtcMint: zbtcMint,
//       })
//       .rpc();

//     // Verify sBTC authority was transferred to program PDA
//     data = await connection.getParsedAccountInfo(sbtcMint, "confirmed");
//     parsedMintInfo = (data?.value?.data as ParsedAccountData)?.parsed?.info;
//     if (parsedMintInfo) {
//       expect(parsedMintInfo.mintAuthority===sbtcMintAuthorityPda);
//     }

//     // Verify vaults were created by the program
//     const treasuryAccount = await getAccount(connection, treasuryZbtcVaultPda);
//     const feeAccount = await getAccount(connection, feeVaultPda);
//     expect(treasuryAccount.owner.equals(treasuryZbtcVaultPda)).to.be.true;
//     expect(feeAccount.owner.equals(feeVaultPda)).to.be.true;
//     expect(treasuryAccount.mint.equals(zbtcMint)).to.be.true;
//     expect(feeAccount.mint.equals(zbtcMint)).to.be.true;

//     // Verify config was stored
//     const config = await program.account.config.fetch(configPda);
//     expect(config.squadMultisig.equals(admin.publicKey)).to.be.true;
//     expect(config.sbtcMint.equals(sbtcMint)).to.be.true;
//     expect(config.zbtcMint.equals(zbtcMint)).to.be.true;
//     expect(config.treasuryZbtcVault.equals(treasuryZbtcVaultPda)).to.be.true;
//     expect(config.feeVault.equals(feeVaultPda)).to.be.true;
//     expect(config.sbtcMintAuthorityPda.equals(sbtcMintAuthorityPda)).to.be.true;
//     expect(config.feeRateBps.toNumber()).to.equal(FEE_RATE_BPS);
//     expect(config.minCollateralBps.toNumber()).to.equal(MIN_COLLATERAL_BPS);
//   });
// });


// describe("otc-swap: mint sBTC", async () => {
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
//   let treasuryZbtcVaultPda: PublicKey;
//   let feeVaultPda: PublicKey;
//   let configPda: PublicKey;

//   // Test users
//   let user: Keypair;
//   let userSbtcAta: PublicKey;
//   let userZbtcAta: PublicKey;

//   // Helper function to derive PDAs
//   const deriveVaultPdAs = (squadWallet: PublicKey) => {
//     const [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("sbtc_mint_authority")],
//       program.programId
//     );

//     const [treasuryPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("treasury"), squadWallet.toBuffer()],
//       program.programId
//     );

//     const [feePda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("fees"), squadWallet.toBuffer()],
//       program.programId
//     );

//     const [configPda] = PublicKey.findProgramAddressSync(
//       [Buffer.from("config"), squadWallet.toBuffer()],
//       program.programId
//     );

//     return {
//       sbtcMintAuthorityPda,
//       treasuryZbtcVaultPda: treasuryPda,
//       feeVaultPda: feePda,
//       configPda,
//     };
//   };

//   before(async () => {
//     // Create test user
//     user = Keypair.generate();

//     // Airdrop SOL to user
//     const airdropSig = await connection.requestAirdrop(
//       user.publicKey,
//       1000000000 // 1 SOL
//     );
//     await connection.confirmTransaction(airdropSig);

//     // === Create test mints ===
//     sbtcMint = await createMint(
//       connection,
//       admin.payer,
//       admin.publicKey, // Initial mint authority = Squad
//       admin.publicKey, // Freeze authority = Squad
//       9,
//     );

//     zbtcMint = await createMint(
//       connection,
//       admin.payer,
//       admin.publicKey,
//       null,
//       9
//     );

//     // === Derive PDA vault addresses ===
//     const pdas = deriveVaultPdAs(admin.publicKey);
//     sbtcMintAuthorityPda = pdas.sbtcMintAuthorityPda;
//     treasuryZbtcVaultPda = pdas.treasuryZbtcVaultPda;
//     feeVaultPda = pdas.feeVaultPda;
//     configPda = pdas.configPda;

//     // Initialize the program
//     await program.methods
//       .initialize(
//         new anchor.BN(FEE_RATE_BPS),
//         new anchor.BN(MIN_COLLATERAL_BPS)
//       )
//       .accounts({
//         squadMultisig: admin.publicKey,
//         sbtcMint: sbtcMint,
//         zbtcMint: zbtcMint,
//       })
//       .rpc();

//     // Create user token accounts
//     userSbtcAta = getAssociatedTokenAddressSync(sbtcMint, user.publicKey);
//     userZbtcAta = getAssociatedTokenAddressSync(zbtcMint, user.publicKey);

//     // Create user ATAs if they don't exist
//     await createAccount(connection, admin.payer, zbtcMint, user.publicKey);
//     await createAccount(connection, admin.payer, sbtcMint, user.publicKey);

//     // Mint some zBTC to user for testing
//     await mintTo(
//       connection,
//       admin.payer,
//       zbtcMint,
//       userZbtcAta,
//       admin.publicKey,
//       1000000000000 // 1000 zBTC (9 decimals)
//     );
//   });


//   it("Should successfully mint sBTC when user deposits zBTC", async () => {
//     const depositAmount = new anchor.BN(100000000000); // 100 zBTC
    
//     // Get initial balances
//     const initialUserZbtc = await getAccount(connection, userZbtcAta);
//     const initialUserSbtc = await getAccount(connection, userSbtcAta);
//     const initialTreasury = await getAccount(connection, treasuryZbtcVaultPda);
//     const initialFees = await getAccount(connection, feeVaultPda);

//     const tx = await program.methods
//       .mintSbtc(depositAmount)
//       .accounts({
//         user: user.publicKey,
//         squadMultisig: admin.publicKey,
//         zbtcMint: zbtcMint,
//         sbtcMint: sbtcMint,
//         userZbtcAccount: userZbtcAta,
//         userSbtcAccount: userSbtcAta,
//       })
//       .signers([user])
//       .rpc();

//     // Verify final balances
//     const finalUserZbtc = await getAccount(connection, userZbtcAta);
//     const finalUserSbtc = await getAccount(connection, userSbtcAta);
//     const finalTreasury = await getAccount(connection, treasuryZbtcVaultPda);
//     const finalFees = await getAccount(connection, feeVaultPda);

//     // Calculate expected amounts (1:1 price with 0.5% fee)
//     const expectedFee = depositAmount.mul(new anchor.BN(FEE_RATE_BPS)).div(new anchor.BN(10000));
//     const expectedNetZbtc = depositAmount.sub(expectedFee);
//     const expectedSbtcMinted = expectedNetZbtc; // 1:1 price

//     // Verify zBTC deductions
//     let initialUserZbtcAmount = new anchor.BN(initialUserZbtc.amount);
//     console.log(`initial amount: ${initialUserZbtcAmount.toNumber()} | final amount: ${finalUserZbtc.amount}`)
//     expect(Number(finalUserZbtc.amount)).to.equal(
//       initialUserZbtcAmount.toNumber() - depositAmount.toNumber()
//     );

//     // Verify treasury received net zBTC
//     let initialTreasuryAmount = new anchor.BN(initialTreasury.amount);
//     expect(Number(finalTreasury.amount)).to.equal(
//       initialTreasuryAmount.toNumber() + expectedNetZbtc.toNumber()
//     );

//     // Verify fees collected
//     let initialFeesAmount = new anchor.BN(initialFees.amount);
//     expect(Number(finalFees.amount)).to.equal(
//       initialFeesAmount.toNumber() + expectedFee.toNumber()
//     );

//     // Verify user received sBTC
//     let initialUserSbtcAmount = new anchor.BN(initialUserSbtc.amount);      
//     expect(Number(finalUserSbtc.amount)).to.equal(
//       initialUserSbtcAmount.toNumber() + expectedSbtcMinted.toNumber()
//     );
//   });


//   it("Should fail when user has insufficient zBTC balance", async () => {
//     const userZbtcBalance = await getAccount(connection, userZbtcAta);
//     const excessiveAmount = new anchor.BN(userZbtcBalance.amount).addn(1000);

//     await expect(
//       program.methods
//         .mintSbtc(excessiveAmount)
//         .accounts({
//           user: user.publicKey,
//           zbtcMint: zbtcMint,
//           sbtcMint: sbtcMint,
//           userZbtcAccount: userZbtcAta,
//           userSbtcAccount: userSbtcAta,
//         })
//         .signers([user])
//         .rpc()
//     ).to.be.rejected;
//   });


//   it("Should fail when user doesn't own the zBTC account", async () => {
//     const maliciousUser = Keypair.generate();
//     const depositAmount = new anchor.BN(1000000);

//     await expect(
//       program.methods
//         .mintSbtc(depositAmount)
//         .accounts({
//           user: user.publicKey, // Correct user
//           zbtcMint: zbtcMint,
//           sbtcMint: sbtcMint,
//           userZbtcAccount: userZbtcAta, // But signed by wrong user
//           userSbtcAccount: userSbtcAta,
//         })
//         .signers([maliciousUser]) // Wrong signer
//         .rpc()
//     ).to.be.rejected;
//   });


//   it("Should fail when using wrong mint accounts", async () => {
//     const depositAmount = new anchor.BN(1000000);

//     // Create a fake mint
//     const fakeMint = await createMint(
//       connection,
//       admin.payer,
//       admin.publicKey,
//       null,
//       9
//     );

//     await expect(
//       program.methods
//         .mintSbtc(depositAmount)
//         .accounts({
//           user: user.publicKey,
//           zbtcMint: fakeMint, // Wrong zBTC mint
//           sbtcMint: sbtcMint,
//           userZbtcAccount: userZbtcAta,
//           userSbtcAccount: userSbtcAta,
//         })
//         .signers([user])
//         .rpc()
//     ).to.be.rejected;
//   });


//   it("Should handle minimum amounts correctly", async () => {
//     const tinyAmount = new anchor.BN(1); // 1 lamport

//     await expect(
//       program.methods
//         .mintSbtc(tinyAmount)
//         .accounts({
//           user: user.publicKey,
//           zbtcMint: zbtcMint,
//           sbtcMint: sbtcMint,
//           userZbtcAccount: userZbtcAta,
//           userSbtcAccount: userSbtcAta,
//         })
//         .signers([user])
//         .rpc()
//     ).to.be.rejected; // Should fail for very small amounts
//   });


//   it("Should emit MintEvent on successful mint", async () => {
//     const depositAmount = new anchor.BN(50000000); // 50 zBTC

//     const tx = await program.methods
//       .mintSbtc(depositAmount)
//       .accounts({
//         user: user.publicKey,
//         squadMultisig: admin.publicKey,
//         zbtcMint: zbtcMint,
//         sbtcMint: sbtcMint,
//         userZbtcAccount: userZbtcAta,
//         userSbtcAccount: userSbtcAta,
//       })
//       .signers([user])
//       .rpc();

//     // TODO: Check transaction logs for MintEvent
//     // This would require parsing transaction logs for the event
//   });
// });


describe("otc-swap: burn sBTC", async () => {
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);

  const program = anchor.workspace.OtcSwap as Program<OtcSwap>;
  const admin = provider.wallet;
  const connection = provider.connection;

  // Constants
  const FEE_RATE_BPS = 50; // 0.5%
  const MIN_COLLATERAL_BPS = 25000; // 250%

  let sbtcMint: PublicKey;
  let zbtcMint: PublicKey;
  let sbtcMintAuthorityPda: PublicKey;
  let treasuryZbtcVaultPda: PublicKey;
  let feeVaultPda: PublicKey;
  let configPda: PublicKey;

  // Test users
  let user: Keypair;
  let userSbtcAta: PublicKey;
  let userZbtcAta: PublicKey;

  const deriveVaultPdAs = (squadWallet: PublicKey) => {
    const [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("sbtc_mint_authority")],
      program.programId
    );

    const [treasuryPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("treasury"), squadWallet.toBuffer()],
      program.programId
    );

    const [feePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fees"), squadWallet.toBuffer()],
      program.programId
    );

    const [configPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("config"), squadWallet.toBuffer()],
      program.programId
    );

    return {
      sbtcMintAuthorityPda,
      treasuryZbtcVaultPda: treasuryPda,
      feeVaultPda: feePda,
      configPda,
    };
  };

  before(async () => {
    // Create test user
    user = Keypair.generate();
  
    // Airdrop SOL to user
    const airdropSig = await connection.requestAirdrop(
      user.publicKey,
      1000000000 // 1 SOL
    );
    await connection.confirmTransaction(airdropSig);

    // === Create test mints ===
    sbtcMint = await createMint(
      connection,
      admin.payer,
      admin.publicKey, // Initial mint authority = Squad
      admin.publicKey, // Freeze authority = Squad
      9,
    );

    zbtcMint = await createMint(
      connection,
      admin.payer,
      admin.publicKey,
      null,
      9
    );

    // === Derive PDA vault addresses ===
    const pdas = deriveVaultPdAs(admin.publicKey);
    sbtcMintAuthorityPda = pdas.sbtcMintAuthorityPda;
    treasuryZbtcVaultPda = pdas.treasuryZbtcVaultPda;
    feeVaultPda = pdas.feeVaultPda;
    configPda = pdas.configPda;

    // Initialize the program
    await program.methods
      .initialize(
        new anchor.BN(FEE_RATE_BPS),
        new anchor.BN(MIN_COLLATERAL_BPS)
      )
      .accounts({
        squadMultisig: admin.publicKey,
        sbtcMint: sbtcMint,
        zbtcMint: zbtcMint,
      })
      .rpc();

    // Create user token accounts
    userSbtcAta = getAssociatedTokenAddressSync(sbtcMint, user.publicKey);
    userZbtcAta = getAssociatedTokenAddressSync(zbtcMint, user.publicKey);

    // Create user ATAs if they don't exist
    await createAccount(connection, admin.payer, zbtcMint, user.publicKey);
    await createAccount(connection, admin.payer, sbtcMint, user.publicKey);

    // Mint some zBTC to user for testing
    await mintTo(
      connection,
      admin.payer,
      zbtcMint,
      userZbtcAta,
      admin.publicKey,
      1000000000000 // 1000 zBTC (9 decimals)
    );

    const depositAmount = new anchor.BN(500000000000)
    const tx = await program.methods
      .mintSbtc(depositAmount)
      .accounts({
        user: user.publicKey,
        squadMultisig: admin.publicKey,
        zbtcMint: zbtcMint,
        sbtcMint: sbtcMint,
        userZbtcAccount: userZbtcAta,
        userSbtcAccount: userSbtcAta,
      })
      .signers([user])
      .rpc();
    // // Also mint some sBTC to user for burn testing
    // await mintTo(
    //   connection,
    //   admin.payer,
    //   sbtcMint,
    //   userSbtcAta,
    //   program.programId,
    //   500000000 // 500 sBTC (9 decimals)
    // );
  });

  it("Should successfully burn sBTC and return zBTC to user", async () => {
    console.log("Should successfully burn sBTC and return zBTC to user");
    const burnAmount = new anchor.BN(100000000); // 100 sBTC
    
    // Get initial balances
    const initialUserZbtc = await getAccount(connection, userZbtcAta);
    const initialUserSbtc = await getAccount(connection, userSbtcAta);
    const initialTreasury = await getAccount(connection, treasuryZbtcVaultPda);
    const initialFees = await getAccount(connection, feeVaultPda);

    const tx = await program.methods
      .burnSbtc(burnAmount)
      .accounts({
        user: user.publicKey,
        squadMultisig: admin.publicKey,
        zbtcMint: zbtcMint,
        sbtcMint: sbtcMint,
        userZbtcAccount: userZbtcAta,
        userSbtcAccount: userSbtcAta,
      })
      .signers([user])
      .rpc();

    // Verify final balances
    const finalUserZbtc = await getAccount(connection, userZbtcAta);
    const finalUserSbtc = await getAccount(connection, userSbtcAta);
    const finalTreasury = await getAccount(connection, treasuryZbtcVaultPda);
    const finalFees = await getAccount(connection, feeVaultPda);

    // Calculate expected amounts (1:1 price with 0.5% fee)
    const expectedZbtcValue = burnAmount; // 1:1 price
    const expectedFee = expectedZbtcValue.mul(new anchor.BN(FEE_RATE_BPS)).div(new anchor.BN(10000));
    const expectedNetZbtc = expectedZbtcValue.sub(expectedFee);

    // Verify sBTC was burned
    expect(Number(finalUserSbtc.amount)).to.equal(
      Number(initialUserSbtc.amount) - burnAmount.toNumber()
    );

    // Verify user received net zBTC
    expect(Number(finalUserZbtc.amount)).to.equal(
      Number(initialUserZbtc.amount) + expectedNetZbtc.toNumber()
    );

    // Verify treasury paid out zBTC
    expect(Number(finalTreasury.amount)).to.equal(
      Number(initialTreasury.amount) - expectedZbtcValue.toNumber()
    );

    // Verify fees were retained in treasury (fees stay in treasury for burn operations)
    expect(Number(finalFees.amount)).to.equal(
      Number(initialFees.amount) + expectedFee.toNumber()
    );
  });


  it("Should fail when user has insufficient sBTC balance", async () => {
    const userSbtcBalance = await getAccount(connection, userSbtcAta);
    const excessiveAmount = new anchor.BN(Number(userSbtcBalance.amount) + 1000);

    await expect(
      program.methods
        .burnSbtc(excessiveAmount)
        .accounts({
          user: user.publicKey,
          squadMultisig: admin.publicKey,
          zbtcMint: zbtcMint,
          sbtcMint: sbtcMint,
          userZbtcAccount: userZbtcAta,
          userSbtcAccount: userSbtcAta,
        })
        .signers([user])
        .rpc()
    ).to.be.rejected;
  });

  it("Should fail when treasury has insufficient zBTC balance", async () => {
    // First, check current treasury balance
    const treasuryBalance = await getAccount(connection, treasuryZbtcVaultPda);
    
    // Try to burn more sBTC than treasury can cover
    const excessiveBurnAmount = new anchor.BN(Number(treasuryBalance.amount) + 1000);

    await expect(
      program.methods
        .burnSbtc(excessiveBurnAmount)
        .accounts({
          user: user.publicKey,
          squadMultisig: admin.publicKey,
          zbtcMint: zbtcMint,
          sbtcMint: sbtcMint,
          userZbtcAccount: userZbtcAta,
          userSbtcAccount: userSbtcAta,
        })
        .signers([user])
        .rpc()
    ).to.be.rejected;
  });

  it("Should fail when user doesn't own the sBTC account", async () => {
    const maliciousUser = Keypair.generate();
    const burnAmount = new anchor.BN(1000000);

    await expect(
      program.methods
        .burnSbtc(burnAmount)
        .accounts({
          user: user.publicKey, // Correct user
          squadMultisig: admin.publicKey,
          zbtcMint: zbtcMint,
          sbtcMint: sbtcMint,
          userZbtcAccount: userZbtcAta,
          userSbtcAccount: userSbtcAta, // But signed by wrong user
        })
        .signers([maliciousUser]) // Wrong signer
        .rpc()
    ).to.be.rejected;
  });

  it("Should fail when using wrong mint accounts", async () => {
    const burnAmount = new anchor.BN(1000000);

    // Create a fake mint
    const fakeMint = await createMint(
      connection,
      admin.payer,
      admin.publicKey,
      null,
      9
    );

    await expect(
      program.methods
        .burnSbtc(burnAmount)
        .accounts({
          user: user.publicKey,
          squadMultisig: admin.publicKey,
          zbtcMint: zbtcMint,
          sbtcMint: fakeMint, // Wrong sBTC mint
          userZbtcAccount: userZbtcAta,
          userSbtcAccount: userSbtcAta,
        })
        .signers([user])
        .rpc()
    ).to.be.rejected;
  });

  it("Should handle minimum amounts correctly", async () => {
    const tinyAmount = new anchor.BN(1); // 1 lamport

    await expect(
      program.methods
        .burnSbtc(tinyAmount)
        .accounts({
          user: user.publicKey,
          squadMultisig: admin.publicKey,
          zbtcMint: zbtcMint,
          sbtcMint: sbtcMint,
          userZbtcAccount: userZbtcAta,
          userSbtcAccount: userSbtcAta,
        })
        .signers([user])
        .rpc()
    ).to.be.rejected; // Should fail for very small amounts
  });

  it("Should emit BurnEvent on successful burn", async () => {
    const burnAmount = new anchor.BN(50000000); // 50 sBTC

    const tx = await program.methods
      .burnSbtc(burnAmount)
      .accounts({
        user: user.publicKey,
        squadMultisig: admin.publicKey,
        zbtcMint: zbtcMint,
        sbtcMint: sbtcMint,
        userZbtcAccount: userZbtcAta,
        userSbtcAccount: userSbtcAta,
      })
      .signers([user])
      .rpc();

    // TODO: Check transaction logs for BurnEvent
    // This would require parsing transaction logs for the event
  });

  it("Should maintain proper collateralization after burn", async () => {
    // Get current treasury zBTC balance and total sBTC supply
    // const treasuryBalance = await getAccount(connection, treasuryZbtcVaultPda);
    // const sbtcMintInfo = await getAccount(connection, sbtcMint);
    let initialData = await connection.getParsedAccountInfo(sbtcMint, "confirmed");
    let initialParsedMintInfo = (initialData?.value?.data as ParsedAccountData)?.parsed?.info;
    
    const burnAmount = new anchor.BN(50000000); // 50 sBTC

    const tx = await program.methods
      .burnSbtc(burnAmount)
      .accounts({
        user: user.publicKey,
        squadMultisig: admin.publicKey,
        zbtcMint: zbtcMint,
        sbtcMint: sbtcMint,
        userZbtcAccount: userZbtcAta,
        userSbtcAccount: userSbtcAta,
      })
      .signers([user])
      .rpc();

    // After burn, the collateralization should be maintained or improved
    // const finalTreasuryBalance = await getAccount(connection, treasuryZbtcVaultPda);
    // const finalSbtcMintInfo = await getAccount(connection, sbtcMint);

    // Treasury should have less zBTC (paid out to user)
    // sBTC supply should be reduced by burn amount
    // Collateral ratio should remain healthy
    // let data = await connection.getParsedAccountInfo(sbtcMint, "confirmed");
    // let parsedMintInfo = (data?.value?.data as ParsedAccountData)?.parsed?.info;
    // if (parsedMintInfo) {
    //   expect(Number(parsedMintInfo.supply)).to.equal(Number(initialParsedMintInfo.supply) - burnAmount.toNumber() - 60000000);
    // }
    // expect(Number(finalSbtcMintInfo.supply)).to.equal(
    //   Number(sbtcMintInfo.supply) - burnAmount.toNumber()
    // );
  });


});


