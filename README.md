# LetsDAT OTC Swap Program

Otc Swap program written using the Anchor framework.
A user can interact with this program to swap zBTC (Zeus-BTC) for sBTC (Stable BTC).
The sBTC price is computed via a Pyth oracle tracking the 1000SMA BTC index, combined with additional regularization terms.

There are two primary swap functionalities:

1. Mint – The user sends zBTC to receive sBTC.

2. Burn – The user sends sBTC to redeem zBTC.

The program is tested via TypeScript (Mocha) unit tests and includes mock oracle programs for devnet and localnet testing.

## 🎯 Overview

This program enables trustless OTC swaps between zBTC and sBTC tokens.
The swap price is computed via an oracle architecture that ensures accurate and up-to-date BTC-based pricing.

### Key Features

- **Mint (zBTC → sBTC):** Deposit zBTC and mint synthetic BTC tokens.

- **Burn (sBTC → zBTC):** Redeem sBTC for underlying zBTC.

- **Oracle-Based Pricing:** Uses Pyth (for zBTC/USD) and a custom sBTC Oracle PDA (for sBTC/USD).

- **Configurable Fees:** Protocol-level BPS fee applied to swaps.

- **Collateralized System:** Enforces a minimum zBTC collateral ratio for solvency.

- **TypeScript Testing:** Comprehensive integration and unit tests with Mocha.

## 📁 Project Structure

```code
programs/
├── otc-swap/        # 🚀 Main production OTC swap program (this repo)
│   ├── src/lib.rs   # Core swap logic (mint, burn, collateral checks)
│   └── Xargo.toml
│
├── sbtc-oracle/     # 🧮 Lightweight oracle PDA for storing sBTC price data
│   └── src/lib.rs
│
└── mock-pyth/       # 🧪 Mock Pyth price feed for local/devnet testing
    └── src/lib.rs

app/
├── initializeSwap.ts    # Initializes OTC config & mint authorities
├── readMockPyth.ts      # Reads mock Pyth zBTC/USD price feed
└── updateOracleData.ts  # Updates or reads sBTC oracle price

```

## Token Roles

```table
| Token                    | Description                                                                                                                                                                                     |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **zBTC (Zeus BTC)**      | Wrapped BTC on Solana, used as collateral.                                                                                                                                                      |
| **sBTC (stable BTC)** | A minted token whose price follows the **sBTC Oracle PDA** (not the market BTC price directly). The oracle defines its  target value (e.g., a smoothed or index-adjusted BTC price). |
```


## 🚀 Quick Start

### Prerequisites

- Rust + Cargo

- Solana CLI

- Anchor CLI

- Node.js + Yarn or npm

### Installation & Testing

```bash
# Clone the repository
git clone https://github.com/comprido96/LetsDAT-Otc-Swap
cd LetsDAT-Otc-Swap

# Build all programs
anchor build

# Deploy for local testing (all programs)
anchor deploy

# Run unit and integration tests
anchor test
```

## 🔧 Development

### Local Development

For localnet or devnet, all three programs are deployed and used:

- **otc-swap** (main program)

- **mock-pyth** (mock BTC price feed)

- **sbtc-oracle** (custom oracle PDA)

```bash
# Build and deploy all programs
anchor build
yarn deploy:local

# Run the full test suite
yarn test:integration

# Optionally run on localnet
anchor test --provider.cluster localnet
```

These local or devnet deployments let you simulate the oracle system without needing real mainnet feeds.

### Mainnet Deployment

On mainnet, only the otc-swap program is deployed — it uses the official Pyth zBTC/USD feed instead of the mock one.

```bash
# Deploy only otc-swap to mainnet
anchor deploy --provider.cluster mainnet --program-name otc-swap
```

## Oracle Architecture (Mainnet vs Devnet)

```table
| Environment           | zBTC Price Source                                                             | sBTC Price Source            | Description                      |
| --------------------- | ----------------------------------------------------------------------------- | ---------------------------- | -------------------------------- |
| **Mainnet**           | Official [Pyth zBTC/USD feed](https://pyth.network/developers/price-feed-ids) | On-chain PDA (`sbtc_oracle`) | Real oracle data via Pyth        |
| **Devnet / Localnet** | Custom `mock-pyth` program                                                    | On-chain PDA (`sbtc_oracle`) | Mock data for controlled testing |
```

## How it works

- On mainnet, the program reads live data from Pyth Network via SolanaPriceAccount::account_info_to_feed().

- On devnet, it automatically falls back to a simplified mock-pyth account structure, so tests can simulate price changes.

Both feeds provide the same fields:
(price, confidence, exponent, publish_time).

## 📊 Program Details
### otc-swap (Main Program)

Implements the core minting and redemption logic:

- Initialize:
  Configures parameters:

  - Fee rate (BPS)

  - Collateral ratio

  - Oracle feed keys (Pyth + sBTC oracle)

  - Treasury and fee vaults

  - Transfers mint authority of sBTC to program PDA

- Mint sBTC:

  - User deposits zBTC

  - Protocol charges fee

  - Reads zBTC price from Pyth (real or mock)

  - Reads sBTC price from the sBTC oracle PDA

  - Calculates mintable sBTC and checks collateral ratio

  - Mints sBTC to user

- Burn sBTC:

  - User sends sBTC for redemption

  - Reads prices from oracles

  - Calculates redeemable zBTC and fee

  - Transfers zBTC from treasury to user

  - Burns sBTC and updates accounting

## Supporting programs

- sbtc-oracle

  Stores:

  - trend_value (sBTC price in cents)

  - last_update timestamp

  You can manually update these fields via updateOracleData.ts for testing price scenarios.

- mock-pyth

  - A minimal clone of the Pyth price feed format used to simulate zBTC/USD prices for devnet and local testing.

  - Used by otc-swap when the real Pyth program isn’t available.

## Core Parameters

```table
| Constant                    | Description                   | Default         |
| --------------------------- | ----------------------------- | --------------- |
| `CONFIG_MAX_FEE_RATE_BPS`   | Max protocol fee              | `500` (5%)      |
| `CONFIG_MIN_COLLATERAL_BPS` | Minimum collateral ratio      | `20,000` (200%) |
| `ORACLE_MAX_AGE`            | Max staleness for price feeds | `300s`          |
```

## Example Workflow (Devnet)

1. Deploy all programs
```bash
anchor deploy
```

2. Initialize the OTC swap config
```bash
ts-node app/initializeSwap.ts
```

3. Update oracle data
```bash
ts-node app/updateOracleData.ts
```

4. Update mock pyth data
```bash
ts-node app/readMockPyth.ts
```

## Testing

The program includes comprehensive TypeScript unit tests using Mocha and Anchor’s testing framework.
```bash
# Run all tests
anchor test
```

## 🤝 Attribution
This project includes modified versions of:

  - sbtc-oracle: Based on Original Repository by [ppoirier-ai](https://github.com/ppoirier-ai/StableBitcoin-PriceOracle)

  - mock-pyth: Inspired by [mock-pyth](https://github.com/tkkinn/mock-pyth)

## License

MIT License © 2025
