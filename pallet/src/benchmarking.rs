use super::*;

use crate::helper::*;

use crate::Pallet as Amm;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{log::info, traits::Get};
use frame_system::RawOrigin;
use sp_runtime::traits::{Bounded, One};
use sp_std::ops::{Add, Sub};

benchmarks! {
	set_fee_to {
		let alice: T::AccountId = account("Alice",0,0);
		let caller: T::AccountId = whitelisted_caller();

		<FeeToSetter<T>>::put(caller.clone());
	}: _(RawOrigin::Signed(caller),alice.clone())
	verify {
		let fee_to = <FeeTo<T>>::get().expect("should not be empty.");
		assert_eq!(fee_to, alice);
	}

	skim {
		let caller: T::AccountId = whitelisted_caller();
	}: _(RawOrigin::Signed(caller.clone()))
	verify {
		let asset_0 = T::Asset0::get();
		let verify_asset_0 = balance_of::<T>(&caller, asset_0);
		assert_eq!(verify_asset_0, T::Balance::zero());

		let asset_1 = T::Asset1::get();
		let verify_asset_1 = balance_of::<T>(&caller, asset_1);
		assert_eq!(verify_asset_1, T::Balance::zero());
	}

	deposit_asset_1{
		let asset_0 = T::Asset0::get();
		let asset_1 = T::Asset1::get();

		let caller: T::AccountId = <FeeToSetter<T>>::get().unwrap();

		let caller_orig0_bal = balance_of::<T>(&caller,asset_0);
		let caller_orig1_bal = balance_of::<T>(&caller,asset_1);

		let deposit_bal = T::Balance::from(10u8);
	}: _(RawOrigin::Signed(caller.clone()), deposit_bal)
	verify {
		let contract = <ContractId<T>>::get().unwrap();
		let contract_result = balance_of::<T>(&contract, asset_0);
		assert_eq!(contract_result, deposit_bal);

		let contract_result = balance_of::<T>(&contract, asset_1);
		assert_eq!(contract_result, deposit_bal);

		let caller_new0_bal = balance_of::<T>(&caller, asset_0);
		assert_eq!(
			caller_new0_bal + deposit_bal,
			caller_orig0_bal
		);

		let caller_new1_bal = balance_of::<T>(&caller, asset_1);
		assert_eq!(
			caller_new1_bal + deposit_bal,
			caller_orig1_bal
		);
	}

	deposit_asset_2{
		let asset_0 = T::Asset0::get();
		let asset_1 = T::Asset1::get();

		let caller: T::AccountId = <FeeToSetter<T>>::get().unwrap();

		let caller_orig0_bal = balance_of::<T>(&caller,asset_0);
		let caller_orig1_bal = balance_of::<T>(&caller,asset_1);

		let deposit_bal = T::Balance::from(10u8);
	}: _(RawOrigin::Signed(caller.clone()), deposit_bal)
	verify {
		let contract = <ContractId<T>>::get().unwrap();
		let contract_result = balance_of::<T>(&contract, asset_0);
		assert_eq!(contract_result, deposit_bal);

		let contract_result = balance_of::<T>(&contract, asset_1);
		assert_eq!(contract_result, deposit_bal);

		let caller_new0_bal = balance_of::<T>(&caller, asset_0);
		assert_eq!(
			caller_new0_bal + deposit_bal,
			caller_orig0_bal
		);

		let caller_new1_bal = balance_of::<T>(&caller, asset_1);
		assert_eq!(
			caller_new1_bal + deposit_bal,
			caller_orig1_bal
		);
	}

	withdraw{
		let asset_0 = T::Asset0::get();
		let asset_1 = T::Asset1::get();

		let caller: T::AccountId = <FeeToSetter<T>>::get().unwrap();
		let origin = RawOrigin::Signed(caller.clone());

		let white_listed: T::AccountId = whitelisted_caller();
		<Amm<T>>::set_fee_to(T::Origin::from(origin.clone()),white_listed).expect("set ToFee should work");

		let deposit_bal = T::Balance::from(10u8);

		<Amm<T>>::deposit_asset_1(T::Origin::from(origin), deposit_bal).expect("deposit should work");

		let caller_orig0_bal = balance_of::<T>(&caller, asset_0);
		let caller_orig1_bal = balance_of::<T>(&caller, asset_1);

		let contract = <ContractId<T>>::get().unwrap();
		let contract_orig0_bal = balance_of::<T>(&contract, asset_0);
		let contract_orig1_bal = balance_of::<T>(&contract, asset_1);

		let (reserve_orig0, reserve_orig1, _) = reserves::<T>();

		let withdrawal_bal = contract_orig0_bal - T::Balance::one();

	}: _(RawOrigin::Signed(caller.clone()), withdrawal_bal)
	verify {
		let caller_new0_bal = balance_of::<T>(&caller, asset_0);
		assert_eq!(
			caller_new0_bal - withdrawal_bal,
			caller_orig0_bal
		);

		let caller_new1_bal = balance_of::<T>(&caller, asset_1);
		assert_eq!(
			caller_new1_bal - withdrawal_bal,
			caller_orig1_bal
		);

		let contract_new0_bal = balance_of::<T>(&contract, asset_0);
		assert_eq!(
			contract_new0_bal,
			contract_orig0_bal - withdrawal_bal
		);

		let contract_new1_bal = balance_of::<T>(&contract, asset_1);
		assert_eq!(
			contract_new1_bal,
			contract_orig1_bal - withdrawal_bal
		);

		let k_last = <KLast<T>>::get();
		assert_eq!(k_last, reserve_orig0 * reserve_orig1);

		let (reserve_new0, reserve_new1, _) = reserves::<T>();
		assert_eq!(reserve_new0, reserve_orig0 - withdrawal_bal);
		assert_eq!(reserve_new1, reserve_orig1 - withdrawal_bal);
	}

	swap_asset_1_for_asset_2{
		let caller: T::AccountId = <FeeToSetter<T>>::get().unwrap();
		let origin = RawOrigin::Signed(caller.clone());

		let deposit_bal = T::Balance::from(10u8);
		<Amm<T>>::deposit_asset_1(T::Origin::from(origin), deposit_bal).expect("deposit should work");

		let asset_0 = T::Asset0::get();
		let asset_1 = T::Asset1::get();

		let caller_orig0_bal = balance_of::<T>(&caller, asset_0);
		let caller_orig1_bal = balance_of::<T>(&caller, asset_1);

		let contract = <ContractId<T>>::get().unwrap();
		let contract_orig0_bal = balance_of::<T>(&contract, asset_0);
		let contract_orig1_bal = balance_of::<T>(&contract, asset_1);

		let (reserve_orig0, reserve_orig1, _) = reserves::<T>();

		let swap_bal = T::Balance::from(5u8);
	}: _(RawOrigin::Signed(caller.clone()), swap_bal)
	verify {
		let caller_new0_bal = balance_of::<T>(&caller, asset_0);
		assert!(caller_new0_bal < caller_orig0_bal);

		let caller_new1_bal = balance_of::<T>(&caller, asset_1);
		assert_eq!(caller_new1_bal, caller_orig1_bal + swap_bal);

		let contract_new0_bal = balance_of::<T>(&contract, asset_0);
		assert!(contract_new0_bal > contract_orig0_bal);

		let contract_new1_bal = balance_of::<T>(&contract, asset_1);
		assert!(contract_new1_bal < contract_orig1_bal);

		let (reserve_0, reserve_1, _) = reserves::<T>();
		assert!(reserve_0 > reserve_1);
	}

		swap_asset_2_for_asset_1{
		let caller: T::AccountId = <FeeToSetter<T>>::get().unwrap();
		let origin = RawOrigin::Signed(caller.clone());

		let deposit_bal = T::Balance::from(10u8);
		<Amm<T>>::deposit_asset_1(T::Origin::from(origin), deposit_bal).expect("deposit should work");

		let asset_0 = T::Asset0::get();
		let asset_1 = T::Asset1::get();

		let caller_orig0_bal = balance_of::<T>(&caller, asset_0);
		let caller_orig1_bal = balance_of::<T>(&caller, asset_1);

		let contract = <ContractId<T>>::get().unwrap();
		let contract_orig0_bal = balance_of::<T>(&contract, asset_0);
		let contract_orig1_bal = balance_of::<T>(&contract, asset_1);

		let (reserve_orig0, reserve_orig1, _) = reserves::<T>();

		let swap_bal = T::Balance::from(5u8);
	}: _(RawOrigin::Signed(caller.clone()), swap_bal)
	verify {
		let caller_new0_bal = balance_of::<T>(&caller, asset_0);
		assert_eq!(caller_new0_bal, caller_orig0_bal + swap_bal);

		let caller_new1_bal = balance_of::<T>(&caller, asset_1);
		assert!(caller_new1_bal < caller_orig1_bal);

		let contract_new0_bal = balance_of::<T>(&contract, asset_0);
		assert!(contract_new0_bal < contract_orig0_bal);

		let contract_new1_bal = balance_of::<T>(&contract, asset_1);
		assert!(contract_new1_bal > contract_orig1_bal);

		let (reserve_0, reserve_1, _) = reserves::<T>();
		assert!(reserve_0 < reserve_1);
	}
}
