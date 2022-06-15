# The AMM Pallet

Currently the supported assets pair is **EUR/USDC**. The **EUR** is represented as **Asset1**, and the **USDC** as **Asset2**.  
All test accounts have a EUR and USDC balance of **_1000^12_** plancks (~_10^24_ Units).

## Tests

To run the unit tests, run `cargo test`.

## Building and running with test chain

This pallet is already configured in this project's [test chain](../testchain). Make sure that the testchain is up and running.

To interact with the pallet, go to the Developer -> [Extrinsics](https://polkadot.js.org/apps/#/extrinsics) page of the Polkadot's App.  
Set the "submit the following extrinsic" field to **ammEURUSDC**.

### Deposit

Like for the [smart contract](https://pendulum.gitbook.io/pendulum-docs/get-started/playground-ui/interacting-with-the-amm#deposit), two extrinsics are available for depositing: _depositAsset1_ and _depositAsset2_.

1. Enter the desired amount you want to deposit in the "amount" field. You have to use at least **_10000_** for the initial deposit though because of the minimum liquidity requirement when initializing a new pair.
2. Click the "Submit Transaction" and then "Sign and Submit".
3. Go to the [Explorer](https://polkadot.js.org/apps/#/explorer) page and check on the "recent events" table on the right side. The following events should appear:
   - ammEURUSDC.Mint
   - ammEURUSDC.Sync
   - ammEURUSDC.Transfer (2x)
   - currencies.Transferred (2x)
   - tokens.Endowed (2x)

### Swap

Similar to the [smart contract](https://pendulum.gitbook.io/pendulum-docs/get-started/playground-ui/interacting-with-the-amm#swap), two extrinsics are available for swapping assets: _swapAsset1ForAsset2_ and _swapAsset2ForAsset1_.

1. Specify how much of the other asset you want to receive in the "amountToReceive" field.
2. Click the "Submit Transaction" and then "Sign and Submit".
3. On the Explorer page, the following events indicate a successful swap:
   - ammEURUSDC.Swap
   - ammEURUSDC.Sync
   - currencies.Transferred (2x)

If the liquidity pool balance is 0 or the amount inputted is larger than the liquidity pool balance, swapping will be unsuccessful:

```
system.ExtrinsicFailed
ammEURUSDC.InsufficientLiquidity
```

### Withdraw

Copying from the [smart contract](https://pendulum.gitbook.io/pendulum-docs/get-started/playground-ui/interacting-with-the-amm#withdraw), the extrinsic _withdraw_ is available for withdrawing assets from the liquidity pool.

To get the liquidity pool balance:

1. Go to [chainstate](https://polkadot.js.org/apps/#/chainstate) of Polkadot's app.
2. In the "selected state query" field, choose **ammEURUSDC**.
3. Choose "lpBalances" on the dropdown box beside the field mentioned above.
4. Enter your account id
5. Click the "+" button beside "lpBalances".
6. At the bottom, a new field will show up: "ammEURUSDC.lpBalances: Option<u128>", which is already the liquidity pool balance.

A successful withdraw will emit these events:

- ammEURUSDC.Burn
- ammEURUSDC.Sync
- currencies.Transferred (2x)
- ammEURUSDC.Transfer (2x)

If the liquidity pool balance is 0, withdrawing will be unsuccessful:

```
system.ExtrinsicFailed
ammEURUSDC.WithdrawWithoutSupply
```

If the amount set is larger than the liquidity pool balance, an error will be thrown:

```
system.ExtrinsicFailed
ammEURUSDC.InsufficientBalance
```

### FeeTo

The _setFeeTo_ extrinsic will **only** work when "using the selected account" field is **`Alice`**. Alice has been hardcoded as the [`fee_to_setter` in the Genesis Config](https://github.com/pendulum-chain/pendulum-amm/blob/629131197c3b94304a100199b476bba0f87cd516/testchain/node/src/chain_spec.rs#L181) of the testchain.  
If other accounts are used, an error will appear:

```
system.ExtrinsicFailed
ammEURUSDC.Forbidden
```
