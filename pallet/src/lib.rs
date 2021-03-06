#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod helper;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mock;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

use codec::{Codec, Decode, Encode, MaxEncodedLen};
use frame_support::dispatch::DispatchResult;
use scale_info::TypeInfo;

use sp_runtime::traits::{AtLeast32BitUnsigned, Zero};
use sp_std::marker::PhantomData;

#[frame_support::pallet]
pub mod pallet {
	use helper::*;

	use super::*;

	use frame_support::{ensure, pallet_prelude::*};
	use frame_system::{ensure_signed, pallet_prelude::*};
	use sp_runtime::traits::{CheckedSub, IntegerSquareRoot};
	use sp_std::fmt::Debug;
	// use substrate_stellar_sdk as stellar;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	// todo: move to config, the assets.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_timestamp::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type WeightInfo: WeightInfo;

		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Codec
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Debug
			+ MaxEncodedLen
			+ TypeInfo
			+ IntegerSquareRoot;

		/// The currency ID type
		type CurrencyId: Parameter
			+ Member
			+ Copy
			+ MaybeSerializeDeserialize
			+ Ord
			+ TypeInfo
			+ MaxEncodedLen;

		type AmmExtension: AmmExtension<
			Self::AccountId,
			Self::CurrencyId,
			Self::Balance,
			Self::Moment,
		>;

		// type AddressConversion: StaticLookup<
		//     Source = <Self as frame_system::Config>::AccountId,
		//     Target = stellar::PublicKey,
		// >;

		#[pallet::constant]
		type MinimumLiquidity: Get<Self::Balance>;

		// a multiplier for the denominator in _mint_fee
		// expected value is 5
		// todo: this needs a proper name
		#[pallet::constant]
		type MintFee: Get<Self::Balance>;

		// a value to multiply to, in the `swap` func.
		// expected value is 3
		#[pallet::constant]
		type BaseFee: Get<Self::Balance>;

		#[pallet::constant]
		type Asset0: Get<Self::CurrencyId>;

		#[pallet::constant]
		type Asset1: Get<Self::CurrencyId>;
	}

	pub trait WeightInfo {
		fn set_fee_to() -> Weight;
		fn skim() -> Weight;
		fn sync() -> Weight;
		fn deposit_asset_1() -> Weight;
		fn deposit_asset_2() -> Weight;
		fn withdraw() -> Weight;
		fn swap_asset_1_for_asset_2() -> Weight;
		fn swap_asset_2_for_asset_1() -> Weight;
	}

	#[pallet::genesis_config]
	pub struct GenesisConfig<T: Config> {
		pub contract_id: Option<T::AccountId>,
		pub zero_account: Option<T::AccountId>,
		pub fee_to_setter: Option<T::AccountId>,
	}

	#[cfg(feature = "std")]
	impl<T: Config> Default for GenesisConfig<T> {
		fn default() -> Self {
			Self { contract_id: None, zero_account: None, fee_to_setter: None }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig<T> {
		fn build(&self) {
			if let Some(contract) = &self.contract_id {
				<PalletAccountId<T>>::put(contract.clone());
			}

			if let Some(address_zero) = &self.zero_account {
				<AddressZero<T>>::put(address_zero.clone());
			}

			if let Some(fee_to_setter) = &self.fee_to_setter {
				<FeeToSetter<T>>::put(fee_to_setter.clone());
			}
		}
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, Default, MaxEncodedLen, TypeInfo)]
	#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
	pub(super) struct BalanceReserves<Balance, Moment> {
		reserve_0: Balance,
		reserve_1: Balance,
		block_timestamp_last: Moment,
	}

	impl<Balance, Moment> BalanceReserves<Balance, Moment> {
		pub(crate) fn new(
			reserve_0: Balance,
			reserve_1: Balance,
			block_timestamp_last: Moment,
		) -> Self {
			Self { reserve_0, reserve_1, block_timestamp_last }
		}
	}

	#[pallet::type_value]
	pub(super) fn ReservesDefault<T: Config>() -> BalanceReserves<T::Balance, T::Moment> {
		BalanceReserves {
			reserve_0: T::Balance::zero(),
			reserve_1: T::Balance::zero(),
			block_timestamp_last: T::Moment::zero(),
		}
	}

	#[pallet::storage]
	pub(super) type Reserves<T: Config> =
		StorageValue<_, BalanceReserves<T::Balance, T::Moment>, ValueQuery, ReservesDefault<T>>;

	pub fn reserves<T: Config>() -> (T::Balance, T::Balance, T::Moment) {
		let res = <Reserves<T>>::get();

		(res.reserve_0, res.reserve_1, res.block_timestamp_last)
	}

	#[pallet::type_value]
	pub(super) fn ZeroDefault<T: Config>() -> T::Balance {
		T::Balance::zero()
	}

	#[pallet::storage]
	#[pallet::getter(fn price_0_cumulative_last)]
	pub(super) type Price0CumulativeLast<T: Config> =
		StorageValue<_, T::Balance, ValueQuery, ZeroDefault<T>>;

	#[pallet::storage]
	#[pallet::getter(fn price_1_cumulative_last)]
	pub(super) type Price1CumulativeLast<T: Config> =
		StorageValue<_, T::Balance, ValueQuery, ZeroDefault<T>>;

	#[pallet::storage]
	#[pallet::getter(fn k_last)]
	pub(super) type KLast<T: Config> = StorageValue<_, T::Balance, ValueQuery, ZeroDefault<T>>;

	#[pallet::storage]
	#[pallet::getter(fn fee_to)]
	pub type FeeTo<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn fee_to_setter)]
	pub type FeeToSetter<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn total_supply)]
	pub type TotalSupply<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	#[pallet::storage]
	#[pallet::getter(fn lp_balances)]
	pub type LpBalances<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, T::Balance, OptionQuery>;

	#[pallet::storage]
	pub(super) type PalletAccountId<T: Config> = StorageValue<_, T::AccountId, OptionQuery>; // what exactly is this? the account id of this pallet kind of but does it make sense?

	#[pallet::storage]
	pub(super) type AddressZero<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	// Pallets use events to inform users when important changes are made.
	// https://docs.substrate.io/v3/runtime/events-and-errors
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A token transfer occurred.
		/// parameters: [from,to,value]
		Transfer {
			from: Option<T::AccountId>,
			to: Option<T::AccountId>,
			value: T::Balance,
		},

		Mint {
			sender: T::AccountId,
			amount_0: T::Balance,
			amount_1: T::Balance,
		},

		Burn {
			sender: T::AccountId,
			to: T::AccountId,
			amount_0: T::Balance,
			amount_1: T::Balance,
		},

		Swap {
			sender: T::AccountId,
			to: T::AccountId,
			amount_0_in: T::Balance,
			amount_1_in: T::Balance,
			amount_0_out: T::Balance,
			amount_1_out: T::Balance,
		},

		Sync {
			reserve_0: T::Balance,
			reserve_1: T::Balance,
		},
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		Forbidden,
		/// Returned if not enough balance to fulfill a request is available.
		InsufficientBalance,
		/// Returned if not enough allowance to fulfill a request is available.
		InsufficientAllowance,
		InsufficientLiquidity,
		InsufficientLiquidityBalance,
		InsufficientBalance0,
		InsufficientBalance1,
		InsufficientLiquidityMinted,
		InsufficientLiquidityBurned,
		InsufficientInputAmount,
		InsufficientOutputAmount,
		InvalidDepositToken,
		InvalidSwapToken,
		InvalidTo,
		InvalidK,
		IdenticalAddress,
		PairExists,
		AddressGenerationFailed,
		WithdrawWithoutSupply,
	}

	// Dispatchable functions allows users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(<T as Config>::WeightInfo::set_fee_to())]
		pub fn set_fee_to(origin: OriginFor<T>, fee_to: T::AccountId) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			ensure!(
				caller == <FeeToSetter<T>>::get().unwrap(), // the read
				Error::<T>::Forbidden
			);

			<FeeTo<T>>::put(fee_to); //the write

			Ok(())
		}

		/// Force balances to match reserves
		/// At this point, the caller is the recipient.
		/// todo: weight
		#[pallet::weight(<T as Config>::WeightInfo::skim())]
		pub fn skim(origin: OriginFor<T>) -> DispatchResult {
			let to = ensure_signed(origin)?;
			let contract = <PalletAccountId<T>>::get().unwrap();
			let reserves = <Reserves<T>>::get();

			let asset_0 = T::Asset0::get();
			let asset_1 = T::Asset1::get();

			let amount_0_calc =
				balance_of::<T>(&contract, asset_0).checked_sub(&reserves.reserve_0);
			if let Some(amount_0) = amount_0_calc {
				transfer_tokens::<T>(&contract, &to, asset_0, amount_0)?;
			}

			let amount_1_calc =
				balance_of::<T>(&contract, asset_1).checked_sub(&reserves.reserve_1);
			if let Some(amount_1) = amount_1_calc {
				transfer_tokens::<T>(&contract, &to, asset_1, amount_1)?;
			}

			Ok(())
		}

		#[pallet::weight(<T as Config>::WeightInfo::sync())]
		pub fn sync(origin: OriginFor<T>) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			let contract = <PalletAccountId<T>>::get().unwrap();
			let reserves = <Reserves<T>>::get();

			let asset_0 = T::Asset0::get();
			let asset_1 = T::Asset1::get();

			let balance_0 = balance_of::<T>(&contract, asset_0);
			let balance_1 = balance_of::<T>(&contract, asset_1);

			_update::<T>(balance_0, balance_1, reserves.reserve_0, reserves.reserve_1);

			Ok(())
		}

		/// Add liquidity
		#[pallet::weight(<T as Config>::WeightInfo::deposit_asset_1())]
		pub fn deposit_asset_1(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let contract = <PalletAccountId<T>>::get().unwrap();
			let reserves = <Reserves<T>>::get();

			let asset_0 = T::Asset0::get();
			let asset_1 = T::Asset1::get();

			let zero = T::Balance::zero();

			let amount_1 = if reserves.reserve_0 == zero && reserves.reserve_1 == zero {
				amount
			} else {
				quote::<T>(amount, reserves.reserve_0, reserves.reserve_1)?
			};

			transfer_tokens::<T>(&caller, &contract, asset_0, amount)?;
			transfer_tokens::<T>(&caller, &contract, asset_1, amount_1)?;

			mint::<T>(&caller, caller.clone())
		}

		#[pallet::weight(<T as Config>::WeightInfo::deposit_asset_2())]
		pub fn deposit_asset_2(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let contract = <PalletAccountId<T>>::get().unwrap();
			let reserves = <Reserves<T>>::get();

			let asset_0 = T::Asset0::get();
			let asset_1 = T::Asset1::get();

			let zero = T::Balance::zero();

			let amount_0 = if reserves.reserve_0 == zero && reserves.reserve_1 == zero {
				amount
			} else {
				quote::<T>(amount, reserves.reserve_1, reserves.reserve_0)?
			};

			transfer_tokens::<T>(&caller, &contract, asset_0, amount_0)?;
			transfer_tokens::<T>(&caller, &contract, asset_1, amount)?;

			mint::<T>(&caller, caller.clone())
		}

		/// Remove Liquidity
		#[pallet::weight(<T as Config>::WeightInfo::withdraw())]
		pub fn withdraw(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let contract = <PalletAccountId<T>>::get().unwrap();

			ensure!(
				<TotalSupply<T>>::get() != T::Balance::zero(),
				Error::<T>::WithdrawWithoutSupply
			);

			_transfer_liquidity::<T>(caller.clone(), contract, amount)?;

			burn::<T>(&caller, caller.clone()).map_err(|e| DispatchError::from(e))
		}

		#[pallet::weight(<T as Config>::WeightInfo::swap_asset_1_for_asset_2())]
		pub fn swap_asset_1_for_asset_2(
			origin: OriginFor<T>,
			amount_to_receive: T::Balance,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let contract = <PalletAccountId<T>>::get().unwrap();
			let reserves = <Reserves<T>>::get();

			let amount_0_in =
				get_amount_in::<T>(amount_to_receive, reserves.reserve_0, reserves.reserve_1)?;

			let asset_0 = T::Asset0::get();

			transfer_tokens::<T>(&caller, &contract, asset_0, amount_0_in)?;

			_swap::<T>(T::Balance::zero(), amount_to_receive, &caller, caller.clone())
				.map_err(|e| DispatchError::from(e))
		}

		#[pallet::weight(<T as Config>::WeightInfo::swap_asset_2_for_asset_1())]
		pub fn swap_asset_2_for_asset_1(
			origin: OriginFor<T>,
			amount_to_receive: T::Balance,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let contract = <PalletAccountId<T>>::get().unwrap();
			let reserves = <Reserves<T>>::get();

			let amount_1_in =
				get_amount_in::<T>(amount_to_receive, reserves.reserve_1, reserves.reserve_0)?;

			let asset_1 = T::Asset1::get();

			transfer_tokens::<T>(&caller, &contract, asset_1, amount_1_in)?;

			_swap::<T>(amount_to_receive, T::Balance::zero(), &caller, caller.clone())
				.map_err(|e| DispatchError::from(e))
		}
	}
}

pub trait AmmExtension<AccountId, CurrencyId, Balance, Moment> {
	fn fetch_balance(owner: &AccountId, asset: CurrencyId) -> Balance;
	fn transfer_balance(
		from: &AccountId,
		to: &AccountId,
		asset: CurrencyId,
		amount: Balance,
	) -> DispatchResult;

	fn moment_to_balance_type(moment: Moment) -> Balance;
}

pub struct AmmExtendedEmpty<T>(PhantomData<T>);

impl<T: Config> AmmExtension<T::AccountId, T::CurrencyId, T::Balance, T::Moment>
	for AmmExtendedEmpty<T>
{
	fn fetch_balance(_owner: &T::AccountId, _asset: T::CurrencyId) -> T::Balance {
		T::Balance::zero()
	}

	fn transfer_balance(
		_from: &T::AccountId,
		_to: &T::AccountId,
		_asset: T::CurrencyId,
		_amount: T::Balance,
	) -> DispatchResult {
		Ok(())
	}

	fn moment_to_balance_type(_moment: T::Moment) -> T::Balance {
		T::Balance::zero()
	}
}
