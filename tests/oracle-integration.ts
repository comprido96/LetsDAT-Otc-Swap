import { expect } from 'chai';
import { SbtcOracle } from '../target/types/sbtc_oracle';
import * as anchor from "@coral-xyz/anchor";
import { BN, Program } from "@coral-xyz/anchor";
import { OtcSwap } from "../target/types/otc_swap";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import {
  createMint,
  getAccount,
  mintTo,
  createAssociatedTokenAccount,
  TOKEN_PROGRAM_ID,
  getOrCreateAssociatedTokenAccount,
} from "@solana/spl-token";


describe("otc-swap-integration", () => {
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);

  const otcProgram = anchor.workspace.OtcSwap as Program<OtcSwap>;
  const oracleProgram = anchor.workspace.SbtcOracle as Program<SbtcOracle>;
  const connection = provider.connection;
  const admin = provider.wallet;

  // Test constants
  const ZBTC_DECIMALS = 8;
  const SBTC_DECIMALS = 8;
  const FEE_RATE_BPS = 500; // 5%
  const MIN_COLLATERAL_BPS = 20000; // 200%

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
});