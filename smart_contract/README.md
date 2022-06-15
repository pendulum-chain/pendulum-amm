<div background="red">
  <h2 align="center">🚧 Under construction 🚧</h2>
  <h3 align="center">This is an early prototype right now. Do not try to run this in production!</h3>
</div>
<br>

# pendulum-amm

Pendulum AMM smart contract. Built with ink!

# Build and run

## Prerequisites

1. Install Rust and Cargo.
   You can find an installation guide [here](https://doc.rust-lang.org/cargo/getting-started/installation.html).

2. Install the necessary dependencies for compiling ink! smart contracts

```
cargo install cargo-dylint dylint-link
cargo install cargo-contract --force --locked
```

## Building the contract

To compile the contract use:

```
cargo contract build
```

## Testing

To run the tests you can use two different commands depending on whether you want to see the output of `debug_println!` messages in your terminal or not.

```
# Run tests without debug logs in console
cargo test
# Run tests with debug logs in console
cargo test -- --nocapture
```
