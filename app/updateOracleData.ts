import * as anchor from "@coral-xyz/anchor";
import { BN } from "bn.js";
import { Program } from "@coral-xyz/anchor";
import { SbtcOracle } from '../target/types/sbtc_oracle';
import { PublicKey } from "@solana/web3.js";

async function fetchBtcPrice() {
  try {
    const response = await fetch('http://localhost:5000/datapoints/store', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({}),
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const data: any = await response.json();
    
    if (data.success && data.data && data.data.datapoint) {
      const btcPrice = data.data.datapoint.btc_price;
      console.log(`Fetched BTC price from API: ${btcPrice}`);
      return btcPrice;
    } else {
      throw new Error('Invalid response format from API');
    }
  } catch (error) {
    console.error('Error fetching BTC price from API:', error.message);
    throw error;
  }
}

async function main() {
  try {
    const provider = anchor.AnchorProvider.env();
    anchor.setProvider(provider);

    const oracleProgram = anchor.workspace.SbtcOracle as Program<SbtcOracle>;
    const admin = provider.wallet;

    const [oracleStatePda,] = PublicKey.findProgramAddressSync(
      [Buffer.from("oracle")], 
      oracleProgram.programId
    );
    console.log(`oracleStatePda: ${oracleStatePda}`);

    // Fetch BTC price from API instead of using CLI argument
    const btcPrice = await fetchBtcPrice();
    
    // Convert to the same format as before (multiply by 100 for cents/decimals)
    const price = new BN(Math.round(btcPrice * 100));
    console.log(`Using price value: ${price.toString()} (${btcPrice} BTC)`);

    // Update the oracle with the fetched price
    await oracleProgram.methods
      .updateTrendTest(price)
      .accounts({
        oracleState: oracleStatePda,
        authority: admin.publicKey,
      } as any)
      .rpc();

    console.log("Oracle updated with API price.");

    // Verify the update
    const oracleState = await oracleProgram.account.oracleState.fetch(oracleStatePda);
    console.log("Oracle trend_value:", oracleState.trendValue.toString());
    
  } catch (error) {
    console.error("Error in main:", error);
    throw error;
  }
}

main().catch(console.error).finally(() => process.exit(0));
