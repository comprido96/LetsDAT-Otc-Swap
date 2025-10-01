# LetsDAT-Otc-Swap

Otc Swap program written using the Anchor framework.
A user can interact with this program to swap zBTC (Zeus-BTC) for sBTC. The swap price is computed via a Pyth oracle which tracks the 1000SMA BTC index (plus some regularization terms).
There are two swap functionalities (mint/burn), the first one where the user sends zBTC, the second one where the user converts sBTC back into zBTC.

The program is tested via Typescript (Mocha) unit tests.

