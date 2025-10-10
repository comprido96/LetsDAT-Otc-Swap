# LetsDAT OTC Swap Program

Otc Swap program written using the Anchor framework.
A user can interact with this program to swap zBTC (Zeus-BTC) for sBTC. The swap price is computed via a Pyth oracle which tracks the 1000SMA BTC index (plus some regularization terms).
There are two swap functionalities (mint/burn), the first one where the user sends zBTC, the second one where the user converts sBTC back into zBTC.

The program is tested via Typescript (Mocha) unit tests.

## 🎯 Overview

This program enables trustless OTC swaps between zBTC and sBTC tokens. The swap price is computed via a Pyth oracle tracking the 1000SMA BTC index with additional regularization terms.

### Key Features
- **Mint**: Convert zBTC to sBTC
- **Burn**: Convert sBTC back to zBTC  
- **Oracle-based Pricing**: Uses Pyth 1000SMA BTC index with regularization
- **Typescript Testing**: Comprehensive Mocha unit tests

## 📁 Project Structure
programs/
├── otc-swap/ # 🚀 Main production OTC swap program
├── sbtc-oracle/ # 🧪 Modified oracle client for local testing
└── mock-pyth/ # 🧪 Mock Pyth price feed for local testing


## 🚀 Quick Start

### Prerequisites
- Rust
- Solana CLI
- Anchor CLI
- Node.js

### Installation & Testing

```bash
# Clone the repository
git clone <repository-url>
cd <project>

# Build all programs
anchor build

# Deploy for local testing (all three programs)
anchor deploy

# Run tests
anchor test
```

## 🔧 Development

### Local Development
For local testing, all three programs are deployed and used:

```bash
# Build and deploy all programs
anchor build
yarn deploy:local

# Run the test suite
yarn test:integration

# Test on specific cluster
anchor test --provider.cluster localnet
```

### Mainnet Deployment
Only the otc-swap program is deployed to mainnet:

```bash
# Deploy only otc-swap to mainnet
anchor deploy --provider.cluster mainnet --program-name otc-swap
```

## 📊 Program Details
### otc-swap (Main Program)
The core OTC swap functionality with:

zBTC ↔ sBTC conversion

Pyth oracle price feeds

SMA-based pricing with regularization

Mint and burn operations

Testing Utilities
sbtc-oracle: Modified oracle client for accurate local testing

mock-pyth: Mock Pyth implementation for reliable test environments

## 🤝 Attribution
This project includes modified versions of:

sbtc-oracle: Based on Original Repository by [ppoirier-ai](https://github.com/ppoirier-ai/StableBitcoin-PriceOracle)

mock-pyth: Inspired by [mock-pyth](https://github.com/tkkinn/mock-pyth)

## 🧪 Testing
The program includes comprehensive Typescript unit tests using Mocha:

```bash
# Run all tests
anchor test
```
