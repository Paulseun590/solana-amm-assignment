# Solana AMM

A constant-product Automated Market Maker built with Rust, Anchor, and
Solana.

## Features

-   Initialize a liquidity pool for two SPL tokens.
-   Deposit liquidity and receive LP tokens.
-   Withdraw liquidity by burning LP tokens.
-   Swap between token X and token Y in either direction.
-   Collect swap fees in basis points.
-   Store fees in separate X and Y treasury accounts.
-   Apply slippage protection to deposits, withdrawals, and swaps.
-   Reject zero-amount operations.

## How It Works

### Initialization

The program creates a configuration PDA, LP token mint, two pool vaults,
a treasury authority PDA, and two treasury token accounts.

The initializer supplies the token mints, seed, fee, and optional
authority. Fees are expressed in basis points. Fees of 10,000 or higher
are rejected.

### Deposits

Users deposit both tokens in the proportion required by the pool
reserves. The program mints LP tokens representing their share of the
pool.

### Withdrawals

Users burn LP tokens to receive their corresponding share of both pool
reserves. Minimum output amounts protect against slippage.

### Swaps

The program supports X-to-Y and Y-to-X swaps. It deducts the configured
fee from the input amount, sends the fee to the corresponding treasury
account, and uses the remaining input for the constant-product
calculation.

## Instructions

  Instruction    Purpose
  -------------- ------------------------------------------
  `initialize`   Creates the pool and treasury accounts.
  `deposit`      Adds liquidity and mints LP tokens.
  `withdraw`     Burns LP tokens and returns pool assets.
  `swap`         Exchanges tokens and collects fees.

## Requirements

-   Rust and Cargo
-   Solana CLI
-   Anchor CLI
-   Node.js

Dependencies include Anchor 1.0.1, Anchor SPL 1.0.1, and
`constant-product-curve`. Tests use LiteSVM.

## Build

``` bash
anchor build
```

## Run Tests

``` bash
cargo test -p amm-video -- --nocapture
```

The test suite covers initialization, invalid fee rejection, deposits,
withdrawals, swaps in both directions, zero-amount swap rejection, and
slippage protection.

## Program ID

``` text
EubBa6J9Tqw6qRLZkFLZ52P5wvK7i4M8s8Vsx7JHr5Ky
```

## Limitations

This is an educational implementation and has not been audited for
production use. Production deployment would require an independent
security review, additional edge-case testing, RPC redundancy,
transaction retry handling, and monitoring.
