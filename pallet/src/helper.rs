#![cfg_attr(not(feature = "std"), no_std)]

use crate::{
	pallet::{
		reserves, AddressZero, BalanceReserves, Config, Error, Event, FeeTo, KLast, LpBalances,
		Pallet, PalletAccountId, Price0CumulativeLast, Price1CumulativeLast, Reserves, TotalSupply,
	},
	AmmExtension,
};
use frame_support::{ensure, traits::Get};
use sp_runtime::DispatchResult;

use sp_runtime::traits::{
	Bounded, CheckedAdd, CheckedDiv, CheckedSub, IntegerSquareRoot, One, Saturating, Zero,
};

use sp_std::{
	cmp,
	ops::{Add, Sub},
};

type FuncResult<T> = Result<(), Error<T>>;

pub fn mint<T: Config>(to: &T::AccountId, caller: T::AccountId) -> DispatchResult {
	let zero = T::Balance::zero();

	let contract = <PalletAccountId<T>>::get().unwrap();
	let (reserve_0, reserve_1, _) = reserves::<T>();

	let asset_0 = T::Asset0::get();
	let asset_1 = T::Asset1::get();

	let balance_0 = balance_of::<T>(&contract, asset_0);
	let balance_1 = balance_of::<T>(&contract, asset_1);

	let amount_0 = balance_0.checked_sub(&reserve_0).unwrap_or(zero);
	let amount_1 = balance_1.checked_sub(&reserve_1).unwrap_or(zero);

	let fee_on = _mint_fee::<T>(reserve_0, reserve_1)?;
	let total_supply = <TotalSupply<T>>::get();

	let liquidity = if total_supply == zero {
		let to_sqrt = amount_0.saturating_mul(amount_1);
		let liquidity = to_sqrt.integer_sqrt().saturating_sub(T::Balance::one());

		let address_zero = <AddressZero<T>>::get().unwrap();

		// permanently lock first liquidity tokens
		_mint::<T>(&address_zero, T::Balance::one());

		liquidity
	} else {
		let res = cmp::min(
			amount_0.saturating_mul(total_supply).checked_div(&reserve_0).unwrap_or(zero),
			amount_1.saturating_mul(total_supply).checked_div(&reserve_1).unwrap_or(zero),
		);

		res
	};

	ensure!(liquidity > zero, Error::<T>::InsufficientLiquidityMinted);

	_mint::<T>(to, liquidity);
	_update::<T>(balance_0, balance_1, reserve_0, reserve_1);

	if fee_on {
		<KLast<T>>::put(reserve_0.saturating_mul(reserve_1));
	}

	<Pallet<T>>::deposit_event(Event::<T>::Mint { sender: caller, amount_0, amount_1 });

	Ok(())
}

pub fn burn<T: Config>(to: &T::AccountId, caller: T::AccountId) -> DispatchResult {
	let zero = T::Balance::zero();

	let contract = <PalletAccountId<T>>::get().unwrap();
	let (reserve_0, reserve_1, _) = reserves::<T>();

	let asset_0 = T::Asset0::get();
	let asset_1 = T::Asset1::get();

	let balance_0 = balance_of::<T>(&contract, asset_0.clone());
	let balance_1 = balance_of::<T>(&contract, asset_1.clone());

	let liquidity = <LpBalances<T>>::get(&contract).unwrap();

	let fee_on = _mint_fee::<T>(reserve_0, reserve_1)?;
	let total_supply = <TotalSupply<T>>::get();

	let amount_0 = {
		let to_div = liquidity.saturating_mul(balance_0);
		to_div.checked_div(&total_supply).unwrap_or(zero)
	};

	let amount_1 = {
		let to_div = liquidity.saturating_mul(balance_1);
		to_div.checked_div(&total_supply).unwrap_or(zero)
	};

	ensure!(amount_0 > zero && amount_1 > zero, Error::<T>::InsufficientLiquidityBurned);

	_burn::<T>(&contract, liquidity)?;

	transfer_tokens::<T>(&contract, to, asset_0.clone(), amount_0)?;
	transfer_tokens::<T>(&contract, to, asset_1.clone(), amount_1)?;

	let balance_0 = balance_of::<T>(&contract, asset_0);
	let balance_1 = balance_of::<T>(&contract, asset_1);

	_update::<T>(balance_0, balance_1, reserve_0, reserve_1);

	if fee_on {
		let k_last = reserve_0.saturating_mul(reserve_1);
		<KLast<T>>::put(k_last);
	}

	<Pallet<T>>::deposit_event(Event::<T>::Burn {
		sender: caller,
		to: to.clone(),
		amount_0,
		amount_1,
	});

	Ok(())
}

pub fn _swap<T: Config>(
	amount_0_out: T::Balance,
	amount_1_out: T::Balance,
	to: &T::AccountId,
	sender: T::AccountId,
) -> DispatchResult {
	let zero = T::Balance::zero();
	let asset_0 = T::Asset0::get();
	let asset_1 = T::Asset1::get();

	ensure!(amount_0_out > zero || amount_1_out > zero, Error::<T>::InsufficientOutputAmount);
	let (reserve_0, reserve_1, _) = reserves::<T>();

	ensure!(
		amount_0_out < reserve_0 && amount_1_out < reserve_1,
		Error::<T>::InsufficientLiquidity
	);

	// optimistically transfe tokens
	let contract = <PalletAccountId<T>>::get().unwrap();

	if amount_0_out > zero {
		transfer_tokens::<T>(&contract, to, asset_0.clone(), amount_0_out)?;
	}

	if amount_1_out > zero {
		transfer_tokens::<T>(&contract, to, asset_1.clone(), amount_1_out)?;
	}

	let balance_0 = balance_of::<T>(&contract, asset_0.clone());
	let balance_1 = balance_of::<T>(&contract, asset_1.clone());

	let amount_0_in = if balance_0 > reserve_0.saturating_sub(amount_0_out) {
		balance_0.saturating_sub(reserve_0.saturating_sub(amount_0_out))
	} else {
		zero
	};

	let amount_1_in = if balance_1 > reserve_1.saturating_sub(amount_1_out) {
		balance_1.saturating_sub(reserve_1.saturating_sub(amount_1_out))
	} else {
		zero
	};

	ensure! {
		amount_0_in > zero || amount_1_in > zero,
		Error::<T>::InsufficientInputAmount
	}

	let multiplier_1000 = T::Balance::from(1000u32);
	let multiplier_3 = T::BaseFee::get();

	let balance_0_adjusted = balance_0
		.saturating_mul(multiplier_1000)
		.saturating_sub(amount_0_in.saturating_mul(multiplier_3));

	let balance_1_adjusted = balance_1
		.saturating_mul(multiplier_1000)
		.saturating_sub(amount_1_in.saturating_mul(multiplier_3));

	let balance = balance_0_adjusted.saturating_mul(balance_1_adjusted);
	let reserve = reserve_0
		.saturating_mul(reserve_1)
		.saturating_mul(multiplier_1000 * multiplier_1000);

	ensure!(balance >= reserve, Error::<T>::InvalidK);

	let balance_0 = balance_of::<T>(&contract, asset_0);
	let balance_1 = balance_of::<T>(&contract, asset_1);

	_update::<T>(balance_0, balance_1, reserve_0, reserve_1);

	<Pallet<T>>::deposit_event(Event::<T>::Swap {
		sender,
		to: to.clone(),
		amount_0_in,
		amount_1_in,
		amount_0_out,
		amount_1_out,
	});

	Ok(())
}

pub fn transfer_tokens<T: Config>(
	from: &T::AccountId,
	to: &T::AccountId,
	asset: T::CurrencyId,
	amount: T::Balance,
) -> DispatchResult {
	let from_balance = balance_of::<T>(from, asset.clone());

	ensure!(from_balance >= amount, Error::<T>::InsufficientBalance);

	//todo: also don't know this weight
	T::AmmExtension::transfer_balance(from, to, asset, amount)
}

pub fn balance_of<T: Config>(owner: &T::AccountId, asset: T::CurrencyId) -> T::Balance {
	//todo: what's the weight of this function call?
	T::AmmExtension::fetch_balance(owner, asset)
}

pub fn _update<T: Config>(
	balance_0: T::Balance,
	balance_1: T::Balance,
	reserve_0: T::Balance,
	reserve_1: T::Balance,
) {
	let zero = T::Balance::zero();
	let (_, _, block_timestamp_last) = reserves::<T>();

	let block_timestamp: T::Moment = pallet_timestamp::Pallet::<T>::now();

	let time_elapsed = T::AmmExtension::moment_to_balance_type(
		overflowing_sub::<T::Moment>(block_timestamp, block_timestamp_last).0,
	);

	block_timestamp
		.checked_sub(&block_timestamp_last)
		.map(|timestamp| T::AmmExtension::moment_to_balance_type(timestamp))
		.unwrap_or(T::Balance::max_value()); // overflow is desired

	let mutate_cumulative_price = |price: &mut T::Balance,
	                               reserve_x: T::Balance,
	                               reserve_y: T::Balance,
	                               time_elapsed: T::Balance| {
		// * never overflows, and + overflow is desired
		let to_add = reserve_x.checked_div(&reserve_y).unwrap_or(zero).saturating_mul(time_elapsed);

		*price = overflowing_add::<T::Balance>(*price, to_add).0;
	};

	if time_elapsed > zero && reserve_0 != zero && reserve_1 != zero {
		<Price0CumulativeLast<T>>::mutate(|price| {
			mutate_cumulative_price(price, reserve_1, reserve_0, time_elapsed);
		});

		<Price1CumulativeLast<T>>::mutate(|price| {
			mutate_cumulative_price(price, reserve_0, reserve_1, time_elapsed);
		});
	}

	let reserve = BalanceReserves::new(balance_0, balance_1, block_timestamp);
	<Reserves<T>>::put(reserve);

	<Pallet<T>>::deposit_event(Event::<T>::Sync { reserve_0, reserve_1 });
}

fn _mint_fee<T: Config>(reserve_0: T::Balance, reserve_1: T::Balance) -> Result<bool, Error<T>> {
	let zero = T::Balance::zero();
	let k_last = <KLast<T>>::get();

	match <FeeTo<T>>::get() {
		Some(fee_to) => {
			if k_last != zero {
				let root_k = {
					let to_sqrt = reserve_0.saturating_mul(reserve_1);
					to_sqrt.integer_sqrt()
				};

				let root_k_last = k_last.integer_sqrt();

				if root_k > root_k_last {
					let total_supply = <TotalSupply<T>>::get();

					let sub_k_last = root_k.saturating_sub(root_k_last);
					let numerator = total_supply.saturating_mul(sub_k_last);

					let denominator =
						root_k.saturating_mul(T::MintFee::get()).saturating_add(root_k_last);

					let liquidity = numerator.checked_div(&denominator).unwrap_or(zero);

					if liquidity > zero {
						_mint::<T>(&fee_to, liquidity);
					}
				}
			}
			Ok(true)
		},
		None => {
			if k_last != zero {
				<KLast<T>>::put(zero);
			}
			Ok(false)
		},
	}
}

fn _mint<T: Config>(to: &T::AccountId, value: T::Balance) {
	<TotalSupply<T>>::mutate(|v| {
		*v = v.saturating_add(value);
	});

	let prev_bal = <LpBalances<T>>::get(to).unwrap_or(T::Balance::zero());
	<LpBalances<T>>::insert(to.clone(), prev_bal.saturating_add(value));

	<Pallet<T>>::deposit_event(Event::<T>::Transfer { from: None, to: Some(to.clone()), value })
}

fn _burn<T: Config>(from: &T::AccountId, value: T::Balance) -> FuncResult<T> {
	<TotalSupply<T>>::mutate(|v| {
		*v = v.saturating_sub(value);
	});

	let prev_bal = <LpBalances<T>>::get(from).ok_or(Error::<T>::Forbidden)?;
	<LpBalances<T>>::insert(from.clone(), prev_bal.saturating_sub(value));

	<Pallet<T>>::deposit_event(Event::<T>::Transfer { from: Some(from.clone()), to: None, value });

	Ok(())
}

pub fn _transfer_liquidity<T: Config>(
	from: T::AccountId,
	to: T::AccountId,
	amount: T::Balance,
) -> FuncResult<T> {
	let zero = T::Balance::zero();

	let from_balance = <LpBalances<T>>::get(&from).unwrap_or(zero);
	ensure!(from_balance >= amount, Error::<T>::InsufficientBalance);

	<LpBalances<T>>::insert(from.clone(), from_balance.saturating_sub(amount));

	let to_balance = <LpBalances<T>>::get(&to).unwrap_or(zero);
	<LpBalances<T>>::insert(to.clone(), to_balance.saturating_add(amount));

	<Pallet<T>>::deposit_event(Event::<T>::Transfer {
		from: Some(from),
		to: Some(to),
		value: amount,
	});

	Ok(())
}

pub fn _get_amount_out<T: Config>(
	amount_in: T::Balance,
	reserve_in: T::Balance,
	reserve_out: T::Balance,
) -> Result<T::Balance, Error<T>> {
	let zero = T::Balance::zero();

	ensure!(amount_in > zero, Error::<T>::InsufficientInputAmount);

	ensure!(reserve_in > zero && reserve_out > zero, Error::<T>::InsufficientLiquidity);

	let amount_in_with_fee = {
		let sub_fee = T::Balance::from(997u32);
		amount_in.saturating_mul(sub_fee)
	};
	let multiplier_1000 = T::Balance::from(1000u32);

	let numerator = amount_in_with_fee.saturating_mul(reserve_out);
	let denominator = reserve_in.saturating_mul(multiplier_1000).saturating_add(amount_in_with_fee);

	Ok(numerator.checked_div(&denominator).unwrap_or(T::Balance::zero()))
}

pub fn get_amount_in<T: Config>(
	amount_out: T::Balance,
	reserve_in: T::Balance,
	reserve_out: T::Balance,
) -> Result<T::Balance, Error<T>> {
	let zero = T::Balance::zero();

	if amount_out <= zero {
		return Err(Error::<T>::InsufficientOutputAmount)
	}

	if reserve_in <= zero || reserve_out <= zero {
		return Err(Error::<T>::InsufficientLiquidity)
	}

	let sub_fee = T::Balance::from(997u32);
	let multiplier_1000 = T::Balance::from(1000u32);

	let numerator = reserve_in
		.saturating_mul(reserve_out)
		.saturating_mul(multiplier_1000);
	let denominator = reserve_out.saturating_sub(amount_out).saturating_mul(sub_fee);

	Ok(numerator
		.checked_div(&denominator)
		.map(|res| res.saturating_add(T::Balance::one()))
		.unwrap_or(T::Balance::zero()))
}

pub fn quote<T: Config>(
	amount_a: T::Balance,
	reserve_a: T::Balance,
	reserve_b: T::Balance,
) -> Result<T::Balance, Error<T>> {
	let zero = T::Balance::zero();

	ensure!(amount_a > zero, Error::<T>::InsufficientInputAmount);

	ensure!(reserve_a > zero && reserve_b > zero, Error::<T>::InsufficientLiquidity);

	let amount_b = amount_a
		.saturating_mul(reserve_b)
		.checked_div(&reserve_a)
		.unwrap_or(T::Balance::zero());
	Ok(amount_b)
}

fn overflowing_add<Integer>(augend: Integer, addend: Integer) -> (Integer, bool)
where
	Integer: Bounded + One + CheckedAdd + Add + Sub<Output = Integer>,
{
	augend.checked_add(&addend).map_or_else(
		|| (augend - (Integer::max_value() - addend) - Integer::one(), true), // when there is an overflow
		|sum| (sum, false), // returns the sum when there is no overflow
	)
}

fn overflowing_sub<Integer>(minuend: Integer, subtrahend: Integer) -> (Integer, bool)
where
	Integer: One + Add<Output = Integer> + Bounded + CheckedSub + Sub<Output = Integer>,
{
	minuend.checked_sub(&subtrahend).map_or_else(
		// when there is an overflow
		|| (Integer::max_value() - subtrahend + minuend + Integer::one(), true),
		// returns the difference when there is no overflow
		|difference| (difference, false),
	)
}

#[test]
fn overflow_test() {
	assert_eq!(overflowing_sub::<u32>(5, 2), (3, false));
	assert_eq!(overflowing_sub::<u32>(100, 100), (0, false));
	assert_eq!(overflowing_sub::<u32>(0, 1), (u32::MAX, true));
	assert_eq!(overflowing_sub::<u32>(0, 2), (u32::MAX - 1, true));
	assert_eq!(overflowing_sub::<u32>(100, u32::MAX), (101, true));
	assert_eq!(overflowing_sub::<u8>(0, 1), (u8::MAX, true));

	assert_eq!(overflowing_add::<u32>(5, 2), (7, false));
	assert_eq!(overflowing_add::<u32>(u32::MAX, 1), (0, true));
	assert_eq!(overflowing_add::<u32>(u32::MAX, 2), (1, true));
	assert_eq!(overflowing_add::<u32>(u32::MAX, 200), (199, true));
	assert_eq!(overflowing_add::<u8>(u8::MAX, 1), (0, true));
}
