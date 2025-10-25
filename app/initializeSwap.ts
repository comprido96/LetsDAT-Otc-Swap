import * as anchor from '@coral-xyz/anchor';
import { Program } from '@coral-xyz/anchor';
import { Connection, Keypair, PublicKey, SystemProgram } from '@solana/web3.js';
import { OtcSwap } from '../target/types/otc_swap';
import { SbtcOracle } from '../target/types/sbtc_oracle';
import { MockPyth } from '../target/types/mock_pyth';
import { BN } from 'bn.js';
import { getMint, getOrCreateAssociatedTokenAccount, TOKEN_PROGRAM_ID } from '@solana/spl-token';

// Initialize provider
const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const connection = provider.connection;
const admin = provider.wallet;

const program = anchor.workspace.OtcSwap as Program<OtcSwap>;
const oracleProgram = anchor.workspace.SbtcOracle as Program<SbtcOracle>;
const mockPyth = anchor.workspace.MockPyth as Program<MockPyth>;

// === Accounts ===
let sbtcMint: anchor.web3.PublicKey = new PublicKey("Eyo8RJWpWkx5sRk2WzCKn57sECDLhRXS2tyChLUwhthJ");
let zbtcMint: anchor.web3.PublicKey = new PublicKey("91AgzqSfXnCq6AJm5CPPHL3paB25difEJ1TfSnrFKrf");

let sbtcMintAuthorityPda: anchor.web3.PublicKey;
let treasuryAuthorityPda: anchor.web3.PublicKey;
let feeAuthorityPda: anchor.web3.PublicKey;
let configPda: anchor.web3.PublicKey;

let treasuryZbtcVault: anchor.web3.PublicKey;
let feeVault: anchor.web3.PublicKey;

let oracleStatePda: anchor.web3.PublicKey;
let pythPriceFeedPda: anchor.web3.PublicKey;

const ZBTC_DECIMALS = 8;
const SBTC_DECIMALS = 8;
const FEE_RATE_BPS = 500; // 5%
const MIN_COLLATERAL_BPS = 20000; // 200%


async function main() {
    // === Derive PDAs ===
    [configPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("config_v1"), admin.publicKey.toBuffer()],
      program.programId
    );

    [sbtcMintAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("sbtc_mint_authority"), admin.publicKey.toBuffer()],
      program.programId
    );

    [treasuryAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("treasury_auth_v1"), admin.publicKey.toBuffer()],
      program.programId
    );

    [feeAuthorityPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("fee_auth_v1"), admin.publicKey.toBuffer()],
      program.programId
    );

    [oracleStatePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("oracle")],
      oracleProgram.programId
    );

    [pythPriceFeedPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("mock_v1")],
      mockPyth.programId
    );

    console.log(`configPda:${configPda} sbtcMintAuthorityPda:${sbtcMintAuthorityPda} treasuryAuthorityPda:${treasuryAuthorityPda} feeAuthorityPda:${feeAuthorityPda}`);
    console.log(`oracleStatePda:${oracleStatePda}`);
    console.log(`pythPriceFeedPda:${pythPriceFeedPda}`);

    // // === Create token accounts ===
    // const treasuryAccount = await getOrCreateAssociatedTokenAccount(
    //     connection,
    //     admin.payer,
    //     zbtcMint,
    //     treasuryAuthorityPda, // PDA as owner
    //     true, // allowOwnerOffCurve - for PDAs
    //     undefined, // confirmOptions
    //     undefined, // programId (uses TOKEN_PROGRAM_ID)
    //     TOKEN_PROGRAM_ID // explicitly specify token program
    // );
    // treasuryZbtcVault = treasuryAccount.address;

    // const feeAccount = await getOrCreateAssociatedTokenAccount(
    //     connection,
    //     admin.payer,
    //     zbtcMint,
    //     feeAuthorityPda,
    //     true,
    //     undefined,
    //     undefined,
    //     TOKEN_PROGRAM_ID
    // );
    // feeVault = feeAccount.address;
    // console.log(`treasuryZbtcVault:${treasuryZbtcVault} feeVault:${feeVault}`);

    // console.log("AYO INITIALIZE");

    // // === Verify sBTC initially owned by admin (squad) ===
    // let sbtcMintInfo = await getMint(connection, sbtcMint, null, TOKEN_PROGRAM_ID);
    // console.log(sbtcMintInfo);

    // console.log("Initializing otcSwap...");
    // const tx = await program.methods
    //   .initialize(
    //     new BN(FEE_RATE_BPS),
    //     new BN(MIN_COLLATERAL_BPS),
    //     pythPriceFeedPda,
    //     oracleStatePda,
    //   )
    //   .accounts({
    //     squadMultisig: admin.publicKey,
    //     sbtcMint: sbtcMint,
    //     zbtcMint: zbtcMint,
    //     sbtcMintAuthorityPda: sbtcMintAuthorityPda,
    //     treasuryAuthorityPda: treasuryAuthorityPda,
    //     feeAuthorityPda: feeAuthorityPda,
    //     treasuryZbtcVault: treasuryZbtcVault,
    //     feeVault: feeVault,
    //     config: configPda,
    //     tokenProgram: TOKEN_PROGRAM_ID,
    //     systemProgram: SystemProgram.programId,
    //   } as any)
    //   .rpc();

    // console.log("Initialize tx:", tx);
}

// Run the script
main().catch(console.error);
