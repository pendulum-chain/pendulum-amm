# Pendulum AMM

This project contains implementations of an automated market maker (AMM) both as a smart contract build with ink! and a pallet.
The AMM is based on the Uniswap V2 protocol and follows the interfaces and calculations of the V2 Solidity contracts very closely.
To be more precise, the implementations are comprised of the contents of the following contracts: [UniswapV2Pair.sol](https://github.com/Uniswap/v2-core/blob/master/contracts/UniswapV2Pair.sol), [UniswapV2Factory.sol](https://github.com/Uniswap/v2-core/blob/master/contracts/UniswapV2Factory.sol), [UniswapV2Library.sol](https://github.com/Uniswap/v2-periphery/blob/master/contracts/libraries/UniswapV2Library.sol) and [UniswapV2Router02.sol](https://github.com/Uniswap/v2-periphery/blob/master/contracts/UniswapV2Router02.sol).
For convenience, the signatures of the user-facing methods were changed and are simpler than the ones of the Uniswap V2 protocol.

## Structure

This repository contains three main directories:

- `pallet` - contains the AMM implementation as a pallet.
- `smart_contract` - contains the AMM implementation as a smart contract.
- `testchain` - a simple standalone Substrate chain.
  The testchain has the necessary pallets for running both the AMM smart contract and pallet (e.g. the contracts, and orml-token pallet) as well as a configured chain extension.

## Build, test and run

The instructions on how to build, test and run the AMM implementations are included in the READMEs of the sub-directories.

## Known limitations

The AMM calculations are limited to integer operations because of the non-deterministic behaviour of floating point arithmetic operations.
To improve the precision of price calculations, Uniswap V2 uses a binary fixed point format [UQ112x122](https://github.com/Uniswap/v2-core/blob/master/contracts/libraries/UQ112x112.sol).
The implementations in this project, however, do not use such a format thus the results of calculations might deviate a bit from the Uniswap V2 protocol.
