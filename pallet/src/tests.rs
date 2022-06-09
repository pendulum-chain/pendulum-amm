use crate::{helper::balance_of, mock, mock::*, reserves, Error, Event, LpBalances, Reserves};
use frame_support::{assert_err, assert_ok};
use std::ops::Index;

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
	let mut event = <frame_system::Pallet<Test>>::events();
	let transfer_event = event.get(expected_event_order).unwrap();

	match &transfer_event.event {
		mock::Event::Amm(Event::Transfer { from: _, to: _, value }) => *value,
		_ => {
			assert!(false);
			0
		},
	}
}

fn swap_test(
	origin: AccountId,
	initial_supply: Balance,
	deposit_amount: Balance,
	swap_amount: Balance,
) {
	add_supply_for_account(origin, initial_supply);

	Amm::deposit_asset_1(Origin::signed(origin), deposit_amount).expect("Deposit should work");

	let user_balance_0_pre_swap = balance_of::<Test>(&origin, ASSET_0);

	Amm::swap_asset_2_for_asset_1(Origin::signed(origin), swap_amount).expect("Swap should work");
	let user_balance_0_post_swap = balance_of::<Test>(&origin, ASSET_0);
	assert_eq!(user_balance_0_post_swap, user_balance_0_pre_swap + swap_amount);

	let user_balance_1_pre_swap = balance_of::<Test>(&origin, ASSET_1);
	Amm::swap_asset_1_for_asset_2(Origin::signed(origin), swap_amount).expect("Swap 2 should work");

	let user_balance_1_post_swap = balance_of::<Test>(&origin, ASSET_1);
	assert_eq!(user_balance_1_post_swap, user_balance_1_pre_swap + swap_amount);
}

fn perform_sync() {
	assert_ok!(Amm::sync(Origin::signed(2)));
	let (reserves_0, reserves_1, _) = reserves::<Test>();

	assert_eq!(reserves_0, 1000);
	assert_eq!(reserves_1, 900);
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
fn deposit_works_for_balanced_pair() {
	new_test_ext().execute_with(|| {
		let to = 2;
		let initial_supply = 1_000;
		add_supply_for_account(to, initial_supply);

		let deposit_amount = 100;
		let user_balance_0_pre_deposit = balance_of::<Test>(&to, ASSET_0);
		let user_balance_1_pre_deposit = balance_of::<Test>(&to, ASSET_1);

		Amm::deposit_asset_1(Origin::signed(to), deposit_amount).expect("deposit should work");

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
		let (contract_balance_0_post_deposit, contract_balance_1_post_deposit, _) =
			reserves::<Test>();
		assert_eq!(contract_balance_0_post_deposit, contract_balance_1_post_deposit);
		assert_eq!(deposit_amount, contract_balance_0_post_deposit);
	})
}

#[test]
fn deposit_works_for_unbalanced_pair() {
	new_test_ext().execute_with(|| {
		let to = 2;
		let initial_supply = 1_000;
		add_supply_for_account(to, initial_supply);

		// execute initial deposit
		let deposit_amount = 100;
		Amm::deposit_asset_1(Origin::signed(to), deposit_amount).expect("deposit should work");

		// swap to make it unbalanced
		Amm::swap_asset_2_for_asset_1(Origin::signed(to), 10).expect("swap should work");

		let user_balance_0_pre_deposit = balance_of::<Test>(&to, ASSET_0);
		let user_balance_1_pre_deposit = balance_of::<Test>(&to, ASSET_1);

		// deposit on unbalanced pair
		Amm::deposit_asset_1(Origin::signed(to), deposit_amount).expect("2nd deposit should work");

		let user_balance_0_post_deposit = balance_of::<Test>(&to, ASSET_0);
		let user_balance_1_post_deposit = balance_of::<Test>(&to, ASSET_1);

		let amount_0_in = user_balance_0_pre_deposit - user_balance_0_post_deposit;
		let amount_1_in = user_balance_1_pre_deposit - user_balance_1_post_deposit;

		assert_eq!(deposit_amount, amount_0_in);
		// expect that amount_0_in is less than amount_1_in because
		// the pair has a ratio of 900:1111 after the swap thus TOKEN_0 is more valuable
		assert_eq!(true, amount_0_in < amount_1_in);

		let reserves = reserves::<Test>();
	})
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

		let mut event = <frame_system::Pallet<Test>>::events();
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
			let mut burn_event = event.last_mut().unwrap();

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
		let mut burn_event = event.last_mut().unwrap();

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
		swap_test(2, 1_000_000, 500, 100);
	})
}

#[test]
fn swap_works_with_large_amount() {
	new_test_ext().execute_with(|| {
		swap_test(
			2,          // origin
			10_000_000, // initial supply
			1_000_000,  // deposit amount
			200_000,    // swap amount
		);
	})
}
