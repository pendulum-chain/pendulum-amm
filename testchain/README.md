## Test Chain

This comprises the node and the runtime, from Parity's [substrate-contracts-node](https://github.com/paritytech/substrate-contracts-node#substrate-contracts-node).

---
To run the test node:
1. In the root folder, execute: `cargo b --release`.
2. Check existence in the __target/release__ folder for `pendulum-test-node`.
3. Then execute: 
```
target/release/pendulum-test-node --dev
```
or
```
cargo run --release -- --dev
```
4. Use [Polkadot's app](https://polkadot.js.org/apps/#/) to check the running chain.
5. Go to **DEVELOPMENT**, and make sure that the _**Local Node**_ is set to `127.0.0.0.1:9944`
