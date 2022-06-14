# The AMM Pallet
 
Currently the supported assets pair is **EUR/USDC**. The **EUR** is represented as **Asset1**, and the **USDC** as **Asset2**.  
All test accounts have a balance of **_1000^12_** .

## Tests
To run the unit tests, run `cargo test`.

## Building and running with test chain
This pallet is already configured in this project's [test chain](../testchain). 
See the [testchain README](../testchain/README.md) on how to run it.  

To interact with the pallet, go to the Developer -> [Extrinsics](https://polkadot.js.org/apps/#/extrinsics) page of the Polkadot's App.   
Make sure the "submit the following extrinsic" field is set to **ammEURUSDC**.

### Deposit

Two extrinsics are available for deposit: _depositAsset1_ and _depositAsset2_.
1. Enter the desired amount you want to deposit in the "amount" field. Ex. 10
2. Click the "Submit Transaction" and then "Sign and Submit".
3. Go to the [Explorer](https://polkadot.js.org/apps/#/explorer) page and check on the "recent events" table on the right side, the following events should appear:
   1. ammEURUSDC.Mint
   2. ammEURUSDC.Sync
   3. ammEURUSDC.Transfer (2x)
   4. currencies.Transferred (2x)
   5. tokens.Endowed (2x)

### Swap

Two extrinsics are available for swapping assets: _swapAsset1ForAsset2_ and _swapAsset2ForAsset1_.  

Specify how much of the other asset you want to receive in the "amountToReceive" field.

The following events are emitted in a successful swap:
1. ammEURUSDC.Swap
2. ammEURUSDC.Sync
3. currencies.Transferred (2x)

### Withdraw

The extrinsic _withdraw_ is available for withdrawing assets from the liquidity pool.

To know the liquidity pool balance:
1. Go to [chainstate](https://polkadot.js.org/apps/#/chainstate) of Polkadot's app.
2. In the "selected state query" field, choose **ammEURUSDC**. 
3. Choose "lpBalances" on the dropdown box beside the field mentioned above.
4. Enter your account id
5. Click the "+" button beside "lpBalances".
6. At the bottom, a new field will show up: "ammEURUSDC.lpBalances: Option<u128>", which is already the liquidity pool balance.

A successful withdraw will emit the following events:
1. ammEURUSDC.Burn
2. ammEURUSDC.Sync
3. currencies.Transferred (2x)
4. ammEURUSDC.Transfer (2x)