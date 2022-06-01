#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

mod helper;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mock;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

use scale_info::TypeInfo;
use codec::{Codec, Encode, Decode, MaxEncodedLen};

use sp_runtime::traits::{AtLeast32BitUnsigned, Zero};
use sp_std::marker::PhantomData;

pub type AssetCode = [u8; 12];
pub type IssuerId = [u8; 32];

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, Default, MaxEncodedLen,TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct Asset {
    code: AssetCode,
    issuer: IssuerId
}

impl Asset {
    pub const fn new(code:AssetCode, issuer: IssuerId) -> Self {
        Self {
            code,
            issuer
        }
    }
}

#[frame_support::pallet]
pub mod pallet {
    use helper::*;

    use super::*;

    use sp_std::{fmt::Debug, vec::Vec};
    use frame_support::{ensure, pallet_prelude::*};
    use frame_system::{ensure_signed, pallet_prelude::*};
    use sp_runtime::traits::{IntegerSquareRoot, CheckedSub};
    // use substrate_stellar_sdk as stellar;


    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_timestamp::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

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

        type AmmExtension: AmmExtension<Self::AccountId,Self::Balance, Self::Moment>;

        // type AddressConversion: StaticLookup<
        //     Source = <Self as frame_system::Config>::AccountId,
        //     Target = stellar::PublicKey,
        // >;

        #[pallet::constant]
        type MinimumLiquidity: Get<Self::Balance>;
        
        #[pallet::constant]
        type Asset0: Get<Asset>;
        
        #[pallet::constant]
        type Asset1: Get<Asset>;

        // a multiplier for the denominator in _mint_fee
        // expected value is 5
        // todo: this needs a proper name
        #[pallet::constant]
        type MintFee: Get<Self::Balance>;

        // a value to substract to, in the `get_amount_out` and `get_amount_in` funcs.
        // expected value is 997
        // todo: this needs a proper name
        #[pallet::constant]
        type SubFee: Get<Self::Balance>;

        // a value to multiply to, in the `get_amount_out`, `get_amount_in`, `swap` funcs.
        // expected value is 1000
        // todo: this needs a proper name
        #[pallet::constant]
        type MulBalance: Get<Self::Balance>;

        // a value to multiply to, in the `swap` func.
        // expected value is 3
        // todo: this needs a proper name
        #[pallet::constant]
        type SwapMulBalance: Get<Self::Balance>;
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub contract_id: Option<T::AccountId>,
        pub zero_account: Option<T::AccountId>,
        pub fee_to_setter: Option<T::AccountId>
    }

    #[cfg(feature = "std")]
    impl <T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                contract_id: None,
                zero_account: None,
                fee_to_setter: None
            }
        }
    }

    #[pallet::genesis_build]
    impl <T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            if let Some(contract) = &self.contract_id {
                <ContractId<T>>::put(contract.clone());
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

    pub fn asset_1<T: Config>() -> Vec<u8> {
        T::Asset0::get().code.to_vec()
    }

    pub fn asset_2<T: Config>() -> Vec<u8> {
        T::Asset1::get().code.to_vec()
    }

    pub fn issuer_1<T: Config>() -> Vec<u8> {
        T::Asset0::get().issuer.to_vec()
    }

    pub fn issuer_2<T: Config>() -> Vec<u8> {
        T::Asset1::get().issuer.to_vec()
    }

    #[pallet::storage]
    #[pallet::getter(fn lp_balances)]
    pub type LpBalances<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId,T::Balance, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn total_supply)]
    pub type TotalSupply<T: Config> = StorageValue<_,T::Balance,ValueQuery>;


    #[pallet::type_value]
    pub(super) fn ZeroDefault<T: Config>() -> T::Balance { T::Balance::zero() }

    #[pallet::storage]
    #[pallet::getter(fn price_0_cumulative_last)]
    pub(super) type Price0CumulativeLast<T: Config> = StorageValue<_,T::Balance,ValueQuery,ZeroDefault<T>>;

    #[pallet::storage]
    #[pallet::getter(fn price_1_cumulative_last)]
    pub(super) type Price1CumulativeLast<T: Config> = StorageValue<_,T::Balance,ValueQuery,ZeroDefault<T>>;

    #[pallet::storage]
    #[pallet::getter(fn k_last)]
    pub(super) type KLast<T: Config> = StorageValue<_,T::Balance,ValueQuery,ZeroDefault<T>>;

    #[derive(Debug,Clone, Encode, Decode, Eq, PartialEq, Default, MaxEncodedLen, TypeInfo)]
    #[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
    pub(super) struct BalanceReserves<Balance,Moment> {
        reserve_0:Balance,
        reserve_1:Balance,
        block_timestamp_last:Moment
    }

    impl <Balance,Moment> BalanceReserves<Balance,Moment> {
        pub(crate) fn new(reserve_0: Balance, reserve_1: Balance, block_timestamp_last: Moment) -> Self {
            Self {
                reserve_0,
                reserve_1,
                block_timestamp_last
            }
        }
    }

    #[pallet::type_value]
    pub(super) fn ReservesDefault<T: Config>() -> BalanceReserves<T::Balance,T::Moment> {
        BalanceReserves {
            reserve_0: T::Balance::zero(),
            reserve_1: T::Balance::zero(),
            block_timestamp_last: T::Moment::zero()
        }
    }

    #[pallet::storage]
    pub(super) type Reserves<T: Config> =
    StorageValue<_,BalanceReserves<T::Balance, T::Moment>,ValueQuery,ReservesDefault<T>>;

    pub fn reserves<T: Config>() -> (T::Balance,T::Balance, T::Moment) {
        let res = <Reserves<T>>::get();

        (res.reserve_0, res.reserve_1, res.block_timestamp_last)
    }

    #[pallet::storage]
    #[pallet::getter(fn fee_to)]
    pub type FeeTo<T: Config> = StorageValue<_,T::AccountId,OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn fee_to_setter)]
    pub type FeeToSetter<T: Config> = StorageValue<_,T::AccountId,OptionQuery>;

    #[pallet::storage]
    pub(super) type ContractId<T: Config> = StorageValue<_,T::AccountId,OptionQuery>;

    #[pallet::storage]
    pub(super) type AddressZero<T: Config> = StorageValue<_,T::AccountId,OptionQuery>;



    // Pallets use events to inform users when important changes are made.
    // https://docs.substrate.io/v3/runtime/events-and-errors
    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {

        /// A token transfer occurred.
        /// parameters: [from,to,value]
        Transferred{
            from: Option<T::AccountId>,
            to: Option<T::AccountId>,
            value: T::Balance
        },

        Minted {
            sender: T::AccountId,
            amount_0: T::Balance,
            amount_1: T::Balance
        },

        Burned {
            sender: T::AccountId,
            to: T::AccountId,
            amount_0: T::Balance,
            amount_1: T::Balance
        },

        Swapped {
            sender: T::AccountId,
            to: T::AccountId,
            amount_0_in: T::Balance,
            amount_1_in:T::Balance,
            amount_0_out: T::Balance,
            amount_1_out: T::Balance
        },

        Synced {
            reserve_0: T::Balance,
            reserve_1: T::Balance
        }
    }

    // Errors inform users that something went wrong.
    #[pallet::error]
    pub enum Error<T> {

        ExtraError,
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

        // -- mod errors
        InvalidStellarKeyEncoding,
        InvalidStellarKeyEncodingLength,
        InvalidStellarKeyChecksum,
        InvalidStellarKeyEncodingVersion,
        AssetCodeTooLong,
        InvalidAssetCodeCharacter,
        InvalidBase32Character,
    }

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {

        /// Force balances to match reserves
        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn skim(origin: OriginFor<T>) -> DispatchResult {
            let to = ensure_signed(origin)?;
            let contract = <ContractId<T>>::get().unwrap();
            let reserves = <Reserves<T>>::get();

            let amount_0_calc = balance_of::<T>(&contract, T::Asset0::get())
                .checked_sub(&reserves.reserve_0);
            if let Some(amount_0) = amount_0_calc {
                transfer_tokens::<T>(&contract,&to,T::Asset0::get(), amount_0)?;
            }

            let amount_1_calc = balance_of::<T>(&contract, T::Asset1::get())
                .checked_sub(&reserves.reserve_1);
            if let Some(amount_1) = amount_1_calc {
                transfer_tokens::<T>(&contract, &to,T::Asset1::get(),amount_1)?;
            }

            Ok(())
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn sync(origin: OriginFor<T>) -> DispatchResult {
            let _ = ensure_signed(origin)?;
            let contract = <ContractId<T>>::get().unwrap();
            let reserves = <Reserves<T>>::get();

            let balance_0 = balance_of::<T>(&contract, T::Asset0::get());
            let balance_1 = balance_of::<T>(&contract, T::Asset1::get());

            _update::<T>(balance_0, balance_1, reserves.reserve_0, reserves.reserve_1);

            Ok(())
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn deposit_asset_1(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let contract = <ContractId<T>>::get().unwrap();
            let reserves = <Reserves<T>>::get();

            let amount_1 = quote::<T>(amount, reserves.reserve_0, reserves.reserve_1)?;

            transfer_tokens::<T>(&caller,&contract, T::Asset0::get(), amount)?;
            transfer_tokens::<T>(&caller,&contract, T::Asset1::get(), amount_1)?;


            mint::<T>(&caller,caller.clone()).map_err(|e| DispatchError::from(e))
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn deposit_asset_2(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let contract = <ContractId<T>>::get().unwrap();
            let reserves = <Reserves<T>>::get();

            let amount_0 = quote::<T>(amount, reserves.reserve_1, reserves.reserve_0)?;

            transfer_tokens::<T>(&caller,&contract, T::Asset0::get(), amount_0)?;
            transfer_tokens::<T>(&caller,&contract, T::Asset1::get(), amount)?;

            mint::<T>(&caller,caller.clone()).map_err(|e| DispatchError::from(e))
        }

        /// Remove Liquidity
        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn withdraw(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let contract = <ContractId<T>>::get().unwrap();

            ensure!(
                <TotalSupply<T>>::get() != T::Balance::zero(),
                Error::<T>::WithdrawWithoutSupply
            );

            _transfer_liquidity::<T>(caller.clone(), contract, amount)?;

            burn::<T>(&caller, caller.clone()).map_err(|e| DispatchError::from(e))
        }


        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn swap_asset_1_for_asset_2(origin: OriginFor<T>, amount_to_receive:T::Balance) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let contract = <ContractId<T>>::get().unwrap();
            let reserves = <Reserves<T>>::get();

            // TODO check if the reserves are in correct order
            let amount_0_in = get_amount_in::<T>(
                amount_to_receive,
                reserves.reserve_0,
                reserves.reserve_1
            )?;

            transfer_tokens::<T>(&caller,&contract, T::Asset0::get(), amount_0_in)?;

            _swap::<T>(amount_to_receive, T::Balance::zero(),&caller,caller.clone())
                .map_err(|e| DispatchError::from(e))
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn swap_asset_2_for_asset_1(origin: OriginFor<T>, amount_to_receive:T::Balance,) -> DispatchResult {
            let caller = ensure_signed(origin)?;
            let contract = <ContractId<T>>::get().unwrap();
            let reserves = <Reserves<T>>::get();

            // TODO check if the reserves are in correct order
            let amount_1_in = get_amount_in::<T>(
                amount_to_receive,
                reserves.reserve_1,
                reserves.reserve_0
            )?;

            transfer_tokens::<T>(&caller,&contract, T::Asset1::get(), amount_1_in)?;

            _swap::<T>( T::Balance::zero(), amount_to_receive, &caller,caller.clone())
                .map_err(|e| DispatchError::from(e))
        }

    }
}

pub trait AmmExtension<AccountId, Balance, Moment> {
    fn fetch_balance(owner: &AccountId, asset: Asset) -> Balance;
    fn transfer_balance(from: &AccountId, to: &AccountId, asset: Asset, amount: Balance);

    fn moment_to_balance_type(moment:Moment) -> Balance;
}

pub struct AmmExtendedEmpty<AccountId,Balance, Moment>(PhantomData<(AccountId, Balance, Moment)>);

impl <AccountId, Balance, Moment> AmmExtension<AccountId,Balance, Moment> for AmmExtendedEmpty<AccountId, Balance, Moment>
    where Balance: Zero {

    fn fetch_balance(_owner: &AccountId, _asset: Asset) -> Balance {
        Balance::zero()
    }

    fn transfer_balance(_from: &AccountId, _to: &AccountId,_asset: Asset, _amount: Balance) {
    }

    fn moment_to_balance_type(_moment: Moment) -> Balance { Balance::zero() }
}