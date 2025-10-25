import * as anchor from '@coral-xyz/anchor';
import { Program } from '@coral-xyz/anchor';
import { Connection, Keypair, PublicKey } from '@solana/web3.js';
import { MockPyth } from '../target/types/mock_pyth';
import { BN } from 'bn.js';

// Initialize provider
const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.MockPyth as Program<MockPyth>;

// Find PDA address for price account
const [priceAccountPda, bump] = PublicKey.findProgramAddressSync(
  [Buffer.from("mock_v1")],
  program.programId
);

async function fetchPythPriceData(): Promise<any> {
  const priceId = '0x3d824c7f7c26ed1c85421ecec8c754e6b52d66a4e45de20a9c9ea91de8b396f9';
  const url = `https://hermes.pyth.network/v2/updates/price/latest?ids[]=${priceId}`;
  
  try {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }
    
    const data: any = await response.json();
    
    // Extract the price data from the parsed array
    if (data.parsed && data.parsed.length > 0) {
      const priceData = data.parsed[0];
      return {
        price: priceData.price.price,
        conf: priceData.price.conf,
        expo: priceData.price.expo,
        publish_time: priceData.price.publish_time,
        ema_price: priceData.ema_price.price,
        ema_conf: priceData.ema_price.conf,
        slot: priceData.metadata.slot,
        proof_available_time: priceData.metadata.proof_available_time,
        prev_publish_time: priceData.metadata.prev_publish_time
      };
    } else {
      throw new Error('No price data found in response');
    }
  } catch (error) {
    console.error('Error fetching Pyth price data:', error);
    throw error;
  }
}

async function setFeedFromPyth() {
  try {
    // Fetch latest price data from Pyth API
    const pythData = await fetchPythPriceData();
    
    console.log('Fetched Pyth data:', pythData);
    
    // Send transaction to set feed with real Pyth data
    const txSignature = await program.methods
      .setFeed(
        new BN(pythData.price),           // price
        new BN(pythData.conf),            // conf
        pythData.expo,                          // expo
        new BN(pythData.publish_time),    // publish_time
        new BN(pythData.ema_price),       // ema_price
        new BN(pythData.ema_conf),        // ema_conf
        new BN(pythData.slot),            // slot
        new BN(pythData.proof_available_time), // proof_available_time
        new BN(pythData.prev_publish_time) // prev_publish_time
      )
      .accounts({
        priceAccount: priceAccountPda,
        authority: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .rpc();
    
    console.log('Transaction signature:', txSignature);
    console.log('Price account updated with real Pyth data');
    
    return pythData;
  } catch (error) {
    console.error('Error setting feed from Pyth data:', error);
    throw error;
  }
}

async function readPriceAccount() {
  try {
    const account = await program.account.priceAccount.fetch(priceAccountPda);
    
    console.log("\n=== Current Price Account Data ===");
    console.log("Price Account PDA:", priceAccountPda.toString());
    console.log("Price:", account.price.toString());
    console.log("Conf:", account.conf.toString());
    console.log("Expo:", account.expo);
    console.log("Publish Time:", account.publishTime.toString());
    console.log("EMA Price:", account.emaPrice.toString());
    console.log("EMA Conf:", account.emaConf.toString());
    console.log("Slot:", account.slot.toString());
    console.log("Proof Available Time:", account.proofAvailableTime.toString());
    console.log("Prev Publish Time:", account.prevPublishTime.toString());
    
    return account;
  } catch (error) {
    console.log("Price account not initialized yet");
    return null;
  }
}

// Function to continuously update price feed (optional)
async function startPriceFeedUpdater(intervalMs: number = 30000) {
  console.log(`Starting price feed updater (interval: ${intervalMs}ms)`);
  
  // Initial update
  await setFeedFromPyth();
  await readPriceAccount();
  
  // Set up periodic updates
  setInterval(async () => {
    try {
      console.log('\n--- Updating price feed ---');
      await setFeedFromPyth();
      await readPriceAccount();
    } catch (error) {
      console.error('Error in periodic update:', error);
    }
  }, intervalMs);
}

// Main execution
async function main() {
  console.log('Mock Pyth Price Feed Updater');
  console.log('Program ID:', program.programId.toString());
  console.log('Price Account PDA:', priceAccountPda.toString());
  
  // Option 1: Single update
  await setFeedFromPyth();
  await readPriceAccount();
  
  // Option 2: Uncomment below for continuous updates
  await startPriceFeedUpdater(3600000); // Update every hour
}

// Run the script
main().catch(console.error);
