#![cfg_attr(not(feature = "std"), no_std)]

use crate::{AmmExtension, Asset};
use crate::pallet::{Config, Error, Event, Pallet, BalanceReserves, Price0CumulativeLast, Price1CumulativeLast, reserves, Reserves, KLast, TotalSupply, FeeTo, LpBalances, ContractId, AddressZero};
use frame_support::traits::Get;

use sp_runtime::traits::{Bounded, CheckedAdd, CheckedDiv,CheckedSub, IntegerSquareRoot, Saturating, Zero, One};

use sp_std::cmp;

type FuncResult<T> = Result<(),Error<T>>;

pub fn mint<T: Config>(to: &T::AccountId, caller: T::AccountId) -> FuncResult<T> {
    let zero = T::Balance::zero();

    let contract = <ContractId<T>>::get().unwrap();
    let (reserve_0,reserve_1,_) = reserves::<T>();

    let balance_0 = balance_of::<T>(&contract,T::Asset0::get());
    let balance_1 = balance_of::<T>(&contract, T::Asset1::get());

    let amount_0 = balance_0.checked_sub(&reserve_0).unwrap_or(zero);
    let amount_1 = balance_1.checked_sub(&reserve_1).unwrap_or(zero);

    let fee_on = _mint_fee::<T>(reserve_0, reserve_1)?;
    let total_supply = <TotalSupply<T>>::get();

    let liquidity = if total_supply == zero {
        let to_sqrt = amount_0.saturating_mul(amount_1);
        let liquidity = to_sqrt.integer_sqrt().saturating_sub(T::MinimumLiquidity::get());

        let address_zero = <AddressZero<T>>::get().unwrap();

        _mint::<T>(&address_zero, T::MinimumLiquidity::get())?;

        liquidity
    } else {
        cmp::min(
            amount_0
                .saturating_mul(total_supply)
                .checked_div(&reserve_0)
                .unwrap_or(zero),
            amount_1
                .saturating_mul(total_supply)
                .checked_div(&reserve_1)
                .unwrap_or(zero)
        )
    };

    if liquidity <= zero {
        return Err(Error::<T>::InsufficientLiquidityMinted)
    }

    _mint::<T>(to,liquidity)?;
    _update::<T>(balance_0, balance_1, reserve_0, reserve_1);

    if fee_on {
        <KLast<T>>::put(reserve_0.saturating_mul(reserve_1));
    }

    <Pallet<T>>::deposit_event(Event::<T>::Minted {
        sender: caller,
        amount_0,
        amount_1
    });

    //Ok(liquidity)
    Ok(())
}

pub fn burn<T: Config>(to: &T::AccountId,caller: T::AccountId)
    -> FuncResult<T> {
    let zero = T::Balance::zero();

    let contract = <ContractId<T>>::get().unwrap();
    let (reserve_0,reserve_1,_) = reserves::<T>();

    let balance_0 = balance_of::<T>(&contract,T::Asset0::get());
    let balance_1 = balance_of::<T>(&contract, T::Asset1::get());

    let liquidity = <LpBalances<T>>::get(to).unwrap();

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

    if !(amount_0 > zero || amount_1 > zero) {
        return Err(Error::<T>::InsufficientLiquidityBurned)
    }

    _burn::<T>(&contract,liquidity)?;

    transfer_tokens::<T>(&contract, to, T::Asset0::get(), amount_0)?;
    transfer_tokens::<T>(&contract, to,  T::Asset1::get(), amount_1)?;

    let balance_0 = balance_of::<T>(&contract,  T::Asset0::get());
    let balance_1 = balance_of::<T>(&contract,  T::Asset1::get());

    _update::<T>(balance_0,balance_1, reserve_0, reserve_1);


    if fee_on {
        let k_last = reserve_0.saturating_mul(reserve_1);
        <KLast<T>>::put(k_last);
    }

    <Pallet<T>>::deposit_event(Event::<T>::Burned {
        sender: caller,
        to: to.clone(),
        amount_0,
        amount_1
    });

    Ok(())
}

pub fn _swap<T: Config>(
    amount_0_out: T::Balance,
    amount_1_out: T::Balance,
    to: &T::AccountId,
    sender: T::AccountId
) -> FuncResult<T> {
    let zero = T::Balance::zero();

    if amount_0_out <= zero || amount_1_out <= zero {
        return Err(Error::<T>::InsufficientOutputAmount)
    }

    let (reserve_0, reserve_1, _) = reserves::<T>();

    if amount_0_out >= reserve_0 || amount_1_out >= reserve_1 {
        return Err(Error::<T>::InsufficientLiquidity)
    }

    // optimistically transfe tokens
    let contract = <ContractId<T>>::get().unwrap();

    if amount_0_out > zero {
        transfer_tokens::<T>(&contract,to,T::Asset0::get(),amount_0_out)?;
    }

    if amount_1_out > zero {
        transfer_tokens::<T>(&contract,to,T::Asset1::get(),amount_1_out)?;
    }

    let balance_0 = balance_of::<T>(&contract,T::Asset0::get());
    let balance_1 = balance_of::<T>(&contract, T::Asset1::get());

    let amount_0_in = if balance_0 > reserve_0.saturating_sub(amount_0_out) {
        balance_0.saturating_sub(reserve_0.saturating_sub(amount_0_out))
    } else { zero };

    let amount_1_in = if balance_1 > reserve_1.saturating_sub(amount_1_out) {
        balance_1.saturating_sub(reserve_1.saturating_sub(amount_1_out))
    } else { zero };


    if amount_0_in <= zero || amount_1_in <= zero {
        return Err(Error::<T>::InsufficientInputAmount)
    }

    let multiplier_1000 = T::MulBalance::get();
    let multiplier_3 = T::SwapMulBalance::get();

    let balance_0_adjusted = balance_0.saturating_mul(multiplier_1000)
        .saturating_sub(amount_0_in.saturating_mul(multiplier_3));

    let balance_1_adjusted = balance_1.saturating_mul(multiplier_1000)
        .saturating_sub(amount_1_in.saturating_mul(multiplier_3));

    let balance = balance_0_adjusted.saturating_mul(balance_1_adjusted);
    let reserve =  reserve_0.saturating_mul(reserve_1)
        .saturating_mul(multiplier_1000 * multiplier_1000);

    if balance > reserve {
        return Err(Error::<T>::InvalidK)
    }

    let balance_0 = balance_of::<T>(&contract, T::Asset0::get());
    let balance_1 = balance_of::<T>(&contract, T::Asset1::get());

    _update::<T>(balance_0, balance_1, reserve_0, reserve_1);


    <Pallet<T>>::deposit_event(Event::<T>::Swapped {
        sender,
        to: to.clone(),
        amount_0_in,
        amount_1_in,
        amount_0_out,
        amount_1_out
    });


    Ok(())
}

pub fn transfer_tokens<T: Config>(
    from: &T::AccountId,
    to: &T::AccountId,
    asset: Asset,
    amount: T::Balance
) -> Result<(),Error<T>> {
    let from_balance = balance_of::<T>(from,asset.clone());
    if from_balance < amount {
        return Err(Error::<T>::InsufficientBalance)
    }

    T::AmmExtension::transfer_balance(from,to,asset,amount);

    Ok(())
}

pub fn balance_of<T: Config>(owner:&T::AccountId, asset:Asset) -> T::Balance {
    T::AmmExtension::fetch_balance(owner,asset)
}

pub fn _update<T: Config >(
    balance_0: T::Balance,
    balance_1: T::Balance,
    reserve_0: T::Balance,
    reserve_1: T::Balance
) {
    let zero = T::Balance::zero();

    let (_,_,block_timestamp_last) = reserves::<T>();


    let block_timestamp: T::Moment = pallet_timestamp::Pallet::<T>::now();
    let time_elapsed = block_timestamp.checked_sub(&block_timestamp_last)
        .map(|timestamp| T::AmmExtension::moment_to_balance_type(timestamp))
        .unwrap_or(T::Balance::max_value()); // overflow is desired

    if time_elapsed > zero && reserve_0 != zero && reserve_1 != zero {
        <Price0CumulativeLast<T>>::mutate(|price| {
            let to_add = reserve_1.checked_div(&reserve_0)
                .unwrap_or(zero).saturating_mul(time_elapsed);

            *price = price.clone().checked_add(&to_add).unwrap_or(zero);
        });

        <Price1CumulativeLast<T>>::mutate(|price| {

            let to_add = reserve_0.checked_div(&reserve_1)
                .unwrap_or(zero).saturating_mul(time_elapsed);

            *price = price.clone().checked_add(&to_add).unwrap_or(zero);
        });
    }

    let reserve = BalanceReserves::new(balance_0, balance_1, block_timestamp);
    <Reserves<T>>::put(reserve);

    <Pallet<T>>::deposit_event(Event::<T>::Synced { reserve_0, reserve_1 });
}

fn _mint_fee<T: Config>(reserve_0: T::Balance, reserve_1: T::Balance) -> Result<bool,Error<T>> {
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

                    let denominator = root_k.saturating_mul(T::MintFee::get())
                        .saturating_add(root_k_last);

                    let liquidity = numerator.checked_div(&denominator)
                        .unwrap_or(zero);

                    if liquidity > zero {
                        _mint::<T>(&fee_to,liquidity)?;
                    }
                }
            }
            Ok(true)
        }
        None  => {
            if k_last != zero { <KLast<T>>::put(zero); }
            Ok(false)
        }
    }
}

fn _mint<T: Config>(to: &T::AccountId, value: T::Balance) -> FuncResult<T>  {
    <TotalSupply<T>>::mutate(|v| { *v += value; });

    <LpBalances<T>>::get(to).map(|balance| {
        <LpBalances<T>>::insert(to.clone(), balance + value);
    })
    .ok_or(Error::<T>::ExtraError)
}


fn _burn<T: Config>(from: &T::AccountId, value: T::Balance) -> FuncResult<T> {
    <TotalSupply<T>>::mutate(|v| { *v -= value; });

    <LpBalances<T>>::get(from).map(|balance| {
        <LpBalances<T>>::insert(from.clone(), balance - value);
    })
    .ok_or(Error::<T>::ExtraError)
}


pub fn _transfer_liquidity<T: Config>(
    from: T::AccountId,
    to: T::AccountId,
    amount: T::Balance
) -> FuncResult<T>  {
    if let Some(from_balance) = <LpBalances<T>>::get(&from) {
        if from_balance < amount {
            return Err(Error::<T>::InsufficientBalance)
        }

        if let Some(to_balance) = <LpBalances<T>>::get(&to) {

            <LpBalances<T>>::insert(from, from_balance - amount);
            <LpBalances<T>>::insert(to,  to_balance + amount);

            <Pallet<T>>::deposit_event(Event::<T>::Transferred {
                from: None,
                to: None,
                value: amount
            });

            return Ok(());
        }
    }

    Err(Error::<T>::ExtraError)
}


pub fn get_amount_out<T: Config>(
    amount_in: T::Balance,
    reserve_in: T::Balance,
    reserve_out: T::Balance
) -> Result<T::Balance,Error<T>> {
    let zero = T::Balance::zero();

    if amount_in <= zero {
       return Err(Error::<T>::InsufficientInputAmount)
    }

    if reserve_in <= zero || reserve_out <= zero {
        return Err(Error::<T>::InsufficientLiquidity)
    }

    let amount_in_with_fee = amount_in.saturating_mul(T::SubFee::get());
    let numerator = amount_in_with_fee.saturating_mul(reserve_out);
    let denominator = reserve_in.saturating_mul(T::MulBalance::get())
        .saturating_add(amount_in_with_fee);

    numerator.checked_div(&denominator).ok_or(Error::<T>::ExtraError)
}


pub fn get_amount_in<T: Config>(
    amount_out: T::Balance,
    reserve_in: T::Balance,
    reserve_out: T::Balance
) -> Result<T::Balance, Error<T>> {
    let zero = T::Balance::zero();

    if amount_out <= zero {
        return Err(Error::<T>::InsufficientOutputAmount)
    }

    if reserve_in <= zero || reserve_out <= zero {
        return Err(Error::<T>::InsufficientLiquidity)
    }

    let numerator = reserve_in.saturating_mul(reserve_out).saturating_mul(T::MulBalance::get());
    let denominator = reserve_out.saturating_sub(amount_out).saturating_mul(T::SubFee::get());

    numerator.checked_div(&denominator)
        .map(|res| res.saturating_add(T::Balance::one()))
        .ok_or(Error::<T>::ExtraError)
}

pub fn quote<T: Config>(
    amount_a: T::Balance,
    reserve_a: T::Balance,
    reserve_b: T::Balance
) -> Result<T::Balance, Error<T>> {
    let zero = T::Balance::zero();

    if amount_a <= zero {
        return Err(Error::<T>::InsufficientInputAmount)
    }

    if reserve_a <= zero && reserve_b <= zero {
        return Err(Error::<T>::InsufficientLiquidity)
    }

    let amount_b = amount_a.saturating_mul(reserve_b);
    amount_b.checked_div(&reserve_a).ok_or(Error::<T>::ExtraError)
}