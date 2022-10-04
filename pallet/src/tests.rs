use crate::{
	helper::balance_of, mock, mock::*, reserves, BalanceReserves, Config, Error, Event, FeeTo,
	FeeToSetter, Reserves,
};
use frame_support::{assert_err, pallet_prelude::DispatchResult};
use frame_system::pallet_prelude::OriginFor;

fn add_supply_for_account(account_id: AccountId, supply: Balance) {
	ASSETSMAP0.with(|assets| {
		let mut assets_map = assets.borrow_mut();
		assets_map.insert(account_id, supply);
	});

	ASSETSMAP1.with(|assets| {
		let mut assets_map = assets.borrow_mut();
		assets_map.insert(account_id, supply);
	});
}

fn gained_lp_from_event(expected_event_order: usize) -> Balance {
	let event = <frame_system::Pallet<Test>>::events();
	let transfer_event = event.get(expected_event_order).unwrap();

	match &transfer_event.event {
		mock::Event::Amm(Event::Transfer { from: _, to: _, value }) => *value,
		_ => {
			assert!(false);
			0
		},
	}
}

fn swap_asset2_with_asset1_test(
	initial_supply: Balance,
	deposit_amount: Balance,
	swap_amount: Balance,
) {
	swap_asset_with_other_asset(
		ASSET_0,
		ASSET_1,
		&Amm::deposit_asset_1,
		&Amm::swap_asset_2_for_asset_1,
		&Amm::swap_asset_1_for_asset_2,
		initial_supply,
		deposit_amount,
		swap_amount,
	)
}

fn swap_asset1_with_asset2_test(
	initial_supply: Balance,
	deposit_amount: Balance,
	swap_amount: Balance,
) {
	swap_asset_with_other_asset(
		ASSET_1,
		ASSET_0,
		&Amm::deposit_asset_2,
		&Amm::swap_asset_1_for_asset_2,
		&Amm::swap_asset_2_for_asset_1,
		initial_supply,
		deposit_amount,
		swap_amount,
	)
}

fn swap_asset_with_other_asset(
	first_asset: <Test as Config>::CurrencyId,
	second_asset: <Test as Config>::CurrencyId,
	deposit_func: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult,
	first_swap_func: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult,
	second_swap_func: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult,

	initial_supply: Balance,
	deposit_amount: Balance,
	swap_amount: Balance,
) {
	let origin = 2;
	add_supply_for_account(origin, initial_supply);
	deposit_func(Origin::signed(origin), deposit_amount).expect("Deposit should work");

	let user_balance_first_pre_swap = balance_of::<Test>(&origin, first_asset);
	first_swap_func(Origin::signed(origin), swap_amount).expect("Swap should work");

	let user_balance_first_post_swap = balance_of::<Test>(&origin, first_asset);
	assert_eq!(user_balance_first_post_swap, user_balance_first_pre_swap + swap_amount);

	let user_balance_second_pre_swap = balance_of::<Test>(&origin, second_asset);
	second_swap_func(Origin::signed(origin), swap_amount).expect("Swap 2 should work");

	let user_balance_second_post_swap = balance_of::<Test>(&origin, second_asset);
	assert_eq!(user_balance_second_post_swap, user_balance_second_pre_swap + swap_amount);
}

fn deposit_works_for_balanced_pair(
	f: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult,
) {
	let to = 2;
	let initial_supply = 100_000;
	add_supply_for_account(to, initial_supply);

	let deposit_amount = 10_000;
	let user_balance_0_pre_deposit = balance_of::<Test>(&to, ASSET_0);
	let user_balance_1_pre_deposit = balance_of::<Test>(&to, ASSET_1);

	f(Origin::signed(to), deposit_amount).expect("deposit should work");

	let user_balance_0_post_deposit = balance_of::<Test>(&to, ASSET_0);
	let user_balance_1_post_deposit = balance_of::<Test>(&to, ASSET_1);

	let amount_0_in = user_balance_0_pre_deposit - user_balance_0_post_deposit;
	let amount_1_in = user_balance_1_pre_deposit - user_balance_1_post_deposit;
	// both balances should decrease equally because the asset pair is 1:1
	// i.e. the user has to pay an equal amount of each token
	assert_eq!(amount_0_in, amount_1_in);
	assert_eq!(user_balance_0_pre_deposit - deposit_amount, user_balance_0_post_deposit);
	assert_eq!(user_balance_1_pre_deposit - deposit_amount, user_balance_1_post_deposit);

	// check contract balances
	let (contract_balance_0_post_deposit, contract_balance_1_post_deposit, _) = reserves::<Test>();
	assert_eq!(contract_balance_0_post_deposit, contract_balance_1_post_deposit);
	assert_eq!(deposit_amount, contract_balance_0_post_deposit);
}

fn deposit_works_for_unbalanced_pair(
	deposit_func: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult,
	swap_func: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult,
	amount_check: &dyn Fn(
		<Test as Config>::Balance,
		<Test as Config>::Balance,
		<Test as Config>::Balance,
	),
) {
	let to = 2;
	let initial_supply = 100_000;
	add_supply_for_account(to, initial_supply);

	// execute initial deposit
	let deposit_amount = 10_000;
	deposit_func(Origin::signed(to), deposit_amount).expect("deposit should work");

	// swap to make it unbalanced
	swap_func(Origin::signed(to), 10).expect("swap should work");

	let user_balance_0_pre_deposit = balance_of::<Test>(&to, ASSET_0);
	let user_balance_1_pre_deposit = balance_of::<Test>(&to, ASSET_1);

	// deposit on unbalanced pair
	deposit_func(Origin::signed(to), deposit_amount).expect("2nd deposit should work");

	let user_balance_0_post_deposit = balance_of::<Test>(&to, ASSET_0);
	let user_balance_1_post_deposit = balance_of::<Test>(&to, ASSET_1);

	let amount_0_in = user_balance_0_pre_deposit - user_balance_0_post_deposit;
	let amount_1_in = user_balance_1_pre_deposit - user_balance_1_post_deposit;

	amount_check(deposit_amount, amount_0_in, amount_1_in)
}
fn deposit_below_minimum_liquidity(
	f: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult,
) {
	let to = 2;
	let initial_supply = 100_000;
	add_supply_for_account(to, initial_supply);

	// execute initial deposit
	let deposit_amount = 10;
	assert_err!(f(Origin::signed(to), deposit_amount), Error::<Test>::InsufficientLiquidityMinted)
}

fn not_enough_balance(f: &dyn Fn(OriginFor<Test>, <Test as Config>::Balance) -> DispatchResult) {
	// execute initial deposit
	let deposit_amount = 10_000;
	assert_err!(f(Origin::signed(2), deposit_amount), Error::<Test>::InsufficientBalance)
}

#[test]
fn balance_of_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(balance_of::<Test>(&3, ASSET_0), 0);
		assert_eq!(balance_of::<Test>(&3, ASSET_1), 0);

		assert_eq!(balance_of::<Test>(&4, ASSET_0), 0);
		assert_eq!(balance_of::<Test>(&4, ASSET_1), 0);
	})
}

#[test]
fn deposit1_works_for_balanced_pair() {
	new_test_ext().execute_with(|| deposit_works_for_balanced_pair(&Amm::deposit_asset_1))
}

#[test]
fn deposit2_works_for_balanced_pair() {
	new_test_ext().execute_with(|| deposit_works_for_balanced_pair(&Amm::deposit_asset_2))
}

#[test]
fn deposit1_works_for_unbalanced_pair() {
	new_test_ext().execute_with(|| {
		// expect that amount_0_in is less than amount_1_in because
		// the pair has a ratio of 900:1111 after the swap thus TOKEN_0 is more valuable
		let amount_check = |deposit_amount: <Test as Config>::Balance,
		                    amount_0_in: <Test as Config>::Balance,
		                    amount_1_in: <Test as Config>::Balance| {
			assert_eq!(deposit_amount, amount_0_in);

			assert_eq!(true, amount_0_in < amount_1_in);
		};

		deposit_works_for_unbalanced_pair(
			&Amm::deposit_asset_1,
			&Amm::swap_asset_2_for_asset_1,
			&amount_check,
		)
	})
}

#[test]
fn deposit2_works_for_unbalanced_pair() {
	new_test_ext().execute_with(|| {
		// expect that amount_1_in is less than amount_0_in because
		// the pair has a ratio of 1111:900 after the swap thus TOKEN_1 is more valuableZ
		let amount_check = |deposit_amount: <Test as Config>::Balance,
		                    amount_0_in: <Test as Config>::Balance,
		                    amount_1_in: <Test as Config>::Balance| {
			assert_eq!(deposit_amount, amount_1_in);

			assert_eq!(true, amount_0_in > amount_1_in);
		};

		deposit_works_for_unbalanced_pair(
			&Amm::deposit_asset_2,
			&Amm::swap_asset_1_for_asset_2,
			&amount_check,
		)
	})
}

#[test]
fn deposit1_below_minimum_liquidity() {
	new_test_ext().execute_with(|| deposit_below_minimum_liquidity(&Amm::deposit_asset_1));
}

#[test]
fn deposit2_below_minimum_liquidity() {
	new_test_ext().execute_with(|| deposit_below_minimum_liquidity(&Amm::deposit_asset_2));
}

#[test]
fn deposit1_not_enough_balance() {
	new_test_ext().execute_with(|| not_enough_balance(&Amm::deposit_asset_1));
}

#[test]
fn deposit2_not_enough_balance() {
	new_test_ext().execute_with(|| not_enough_balance(&Amm::deposit_asset_2));
}

#[test]
fn withdraw_without_lp_fails() {
	new_test_ext().execute_with(|| {
		let origin_to = 2;
		let initial_supply = 1_000_000;
		add_supply_for_account(origin_to, initial_supply);

		assert_err!(
			Amm::withdraw(Origin::signed(origin_to), 1),
			Error::<Test>::WithdrawWithoutSupply
		);

		System::set_block_number(1); // to initialize the system, generating the events

		Amm::deposit_asset_1(Origin::signed(origin_to), 5_000).expect("deposit should work");

		let event = <frame_system::Pallet<Test>>::events();
		let transfer_event = event.get(1).unwrap();

		match &transfer_event.event {
			mock::Event::Amm(Event::Transfer { from: _, to: _, value }) => {
				let gained_lp = *value;
				assert_err!(
					// try withdrawing more LP than account has
					Amm::withdraw(Origin::signed(origin_to), gained_lp + 2),
					Error::<Test>::InsufficientBalance
				)
			},
			_ => assert!(false),
		}
	})
}

#[test]
fn withdraw_works() {
	new_test_ext().execute_with(|| {
		let origin_to = 2;
		let initial_supply = 1_000_000;
		add_supply_for_account(origin_to, initial_supply);

		System::set_block_number(1); // to initialize the system, generating the events

		let deposit_amount = 50_000;
		Amm::deposit_asset_1(Origin::signed(origin_to), deposit_amount)
			.expect("deposit should work");

		let user_balance_0_pre_withdraw = balance_of::<Test>(&origin_to, ASSET_0);
		let user_balance_1_pre_withdraw = balance_of::<Test>(&origin_to, ASSET_1);

		let gained_lp = gained_lp_from_event(1);
		Amm::withdraw(Origin::signed(origin_to), gained_lp - 1).expect("withdraw should work");

		let (amount_0, amount_1) = {
			let mut event = <frame_system::Pallet<Test>>::events();
			let burn_event = event.last_mut().unwrap();

			match &burn_event.event {
				mock::Event::Amm(Event::Burn { sender: _, to: _, amount_0, amount_1 }) =>
					(*amount_0, *amount_1),
				_ => {
					assert!(false);
					(0, 0)
				},
			}
		};

		let user_balance_0_post_withdraw = balance_of::<Test>(&origin_to, ASSET_0);
		let user_balance_1_post_withdraw = balance_of::<Test>(&origin_to, ASSET_1);

		assert_eq!(user_balance_0_post_withdraw, user_balance_0_pre_withdraw + amount_0);
		assert_eq!(user_balance_1_post_withdraw, user_balance_1_pre_withdraw + amount_1);
	})
}

#[test]
fn deposit_and_withdraw_work() {
	new_test_ext().execute_with(|| {
		let origin_to = 2;
		let initial_supply = 10_000_000;
		add_supply_for_account(origin_to, initial_supply);

		System::set_block_number(1);

		let deposit_amount = 500_000;
		// do initial deposit which initiates total_supply
		Amm::deposit_asset_1(Origin::signed(origin_to), deposit_amount)
			.expect("Deposit should work");

		// do second deposit
		Amm::deposit_asset_1(Origin::signed(origin_to), deposit_amount)
			.expect("2nd deposit should work");
		let gained_lp = gained_lp_from_event(4);

		Amm::withdraw(Origin::signed(origin_to), gained_lp).expect("withdraw should work");

		let mut event = <frame_system::Pallet<Test>>::events();
		let burn_event = event.last_mut().unwrap();

		match &burn_event.event {
			mock::Event::Amm(Event::Burn { sender: _, to: _, amount_0, amount_1 }) => {
				assert_eq!(
					amount_0, &deposit_amount,
					"expected withdrawn amount_0 to be == to deposited amount"
				);
				assert_eq!(
					amount_1, &deposit_amount,
					"expected withdrawn amount_1 to be == to deposited amount"
				);
			},
			_ => {
				assert!(false);
			},
		}
	})
}

#[test]
fn swap_works_with_small_amount() {
	new_test_ext().execute_with(|| {
		swap_asset2_with_asset1_test(1_000_000, 5000, 100);
	});

	new_test_ext().execute_with(|| {
		swap_asset1_with_asset2_test(1_000_000, 5000, 100);
	})
}

#[test]
fn swap_works_with_large_amount() {
	new_test_ext().execute_with(|| {
		swap_asset2_with_asset1_test(
			// origin
			10_000_000, // initial supply
			1_000_000,  // deposit amount
			200_000,    // swap amount
		);
	});

	new_test_ext().execute_with(|| {
		swap_asset1_with_asset2_test(
			// origin
			10_000_000, // initial supply
			1_000_000,  // deposit amount
			200_000,    // swap amount
		);
	})
}

#[test]
fn swap_fails() {
	new_test_ext().execute_with(|| {
		assert_err!(
			Amm::swap_asset_2_for_asset_1(Origin::signed(2), 1000),
			Error::<Test>::InsufficientLiquidity
		);

		assert_err!(
			Amm::swap_asset_1_for_asset_2(Origin::signed(2), 1000),
			Error::<Test>::InsufficientLiquidity
		);

		assert_err!(
			Amm::swap_asset_1_for_asset_2(Origin::signed(2), 0),
			Error::<Test>::InsufficientOutputAmount
		);

		assert_err!(
			Amm::swap_asset_2_for_asset_1(Origin::signed(2), 0),
			Error::<Test>::InsufficientOutputAmount
		);
	})
}

#[test]
fn set_fee_to_works() {
	new_test_ext().execute_with(|| {
		let fee_to_setter = <FeeToSetter<Test>>::get().expect("should return a value");
		let other_account = 3;

		let orig_fee_to = <FeeTo<Test>>::get();
		assert!(orig_fee_to.is_none());

		System::set_block_number(1);
		Amm::set_fee_to(Origin::signed(fee_to_setter), other_account).expect("should not fail");

		let new_fee_to = <FeeTo<Test>>::get();
		assert_eq!(new_fee_to, Some(other_account));
	})
}

#[test]
fn set_fee_to_fails() {
	new_test_ext().execute_with(|| {
		let other = 3;

		let orig_fee_to = <FeeTo<Test>>::get();
		assert!(orig_fee_to.is_none());

		assert_err!(Amm::set_fee_to(Origin::signed(other), other), Error::<Test>::Forbidden);
		System::set_block_number(1);

		let new_fee_to = <FeeTo<Test>>::get();
		assert_eq!(new_fee_to, orig_fee_to);
	})
}

#[test]
fn set_fee_to_fails_with_no_config() {
	incomplete_config_test_ext().execute_with(|| {
		let fee_setter = 2;
		let to = 3;

		let orig_fee_to = <FeeTo<Test>>::get();
		assert!(orig_fee_to.is_none());

		assert_err!(
			Amm::set_fee_to(Origin::signed(fee_setter), to),
			Error::<Test>::InvalidConfigNoFeeToSetter
		);
	})
}

#[test]
fn skim_works() {
	new_test_ext().execute_with(|| {
		let contract = 1;
		let initial_supply = 100_000;
		add_supply_for_account(contract, initial_supply);

		let caller = 2;

		let asset_0 = <Test as Config>::Asset0::get();
		let contract_asset_0 = balance_of::<Test>(&contract, asset_0);
		let caller_asset_0 = balance_of::<Test>(&caller, asset_0);
		assert_ne!(contract_asset_0, caller_asset_0);

		let asset_1 = <Test as Config>::Asset1::get();
		let contract_asset_1 = balance_of::<Test>(&contract, asset_1);
		let caller_asset_1 = balance_of::<Test>(&caller, asset_1);
		assert_ne!(contract_asset_1, caller_asset_1);

		Amm::skim(Origin::signed(caller)).expect("this shouldn't fail");
		System::set_block_number(1);

		let updated_caller_asset_0 = balance_of::<Test>(&caller, asset_0);
		assert_eq!(updated_caller_asset_0, contract_asset_0);

		let updated_caller_asset_1 = balance_of::<Test>(&caller, asset_1);
		assert_eq!(updated_caller_asset_1, contract_asset_1);
	})
}

#[test]
fn skim_fails() {
	incomplete_config_test_ext().execute_with(|| {
		let caller = 2;
		assert_err!(
			Amm::skim(Origin::signed(caller)),
			Error::<Test>::InvalidConfigNoContractAccount
		);
	})
}

#[test]
fn sync_works() {
	new_test_ext().execute_with(|| {
		let contract = 1;
		let caller = 2;

		let initial_supply = 100_000;
		add_supply_for_account(contract, initial_supply);

		let orig_bal = BalanceReserves::new(10u8.into(), 5u8.into(), 2u8.into());
		<Reserves<Test>>::put(orig_bal);

		Timestamp::set_timestamp(3u8.into());
		System::set_block_number(1);

		let (r_orig0, r_orig1, time_orig) = reserves::<Test>();
		println!("r_orig0: {:?}, r_orig1: {:?}, time_orig: {:?}", r_orig0, r_orig1, time_orig);

		Amm::sync(Origin::signed(caller)).expect("should work");

		let asset_0 = <Test as Config>::Asset0::get();
		let asset_1 = <Test as Config>::Asset1::get();
		let balance_0 = balance_of::<Test>(&contract, asset_0);
		let balance_1 = balance_of::<Test>(&contract, asset_1);

		let (r_new0, r_new1, time_new) = reserves::<Test>();

		assert_ne!(r_new0, r_orig0);
		assert_eq!(r_new0, balance_0);

		assert_ne!(r_new1, r_orig1);
		assert_eq!(r_new1, balance_1);

		assert!(time_new > time_orig);
	})
}

#[test]
fn sync_fails_with_no_config() {
	incomplete_config_test_ext().execute_with(|| {
		assert_err!(Amm::sync(Origin::signed(3)), Error::<Test>::InvalidConfigNoContractAccount);
	})
}
