#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use codec::{Codec, Encode, Decode, MaxEncodedLen};


use sp_runtime::traits::{AtLeast32BitUnsigned, Zero};
use sp_std::marker::PhantomData;

pub use frame_system::pallet::*;

pub type AssetCode = [u8; 12];
pub type IssuerId = [u8; 32]; // encoded 32-bit array of 56 character stellar issuer (public key)

#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq, Default, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize, scale_info::TypeInfo))]
pub struct Asset {
    code: AssetCode,
    issuer: IssuerId
}

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use std::fmt::Debug;
    use frame_support::{ensure, pallet_prelude::*};
    use frame_system::{ensure_signed, pallet_prelude::*};
    use sp_runtime::DispatchResultWithInfo;
    use sp_runtime::traits::{IntegerSquareRoot};
    use sp_std::cmp;


    /// Configure the pallet by specifying the parameters and types on which it depends.
    #[pallet::config]
    pub trait Config: frame_system::Config {
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

        type BalanceExtension: AmmExtended<Self::AccountId,Self::Balance>;

        #[pallet::constant]
        type MinimumLiquidity: Get<Self::Balance>;
        
        #[pallet::constant]
        type Asset1: Get<Asset>;
        
        #[pallet::constant]
        type Asset2: Get<Asset>;
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        contract_id: Option<T::AccountId>
    }

    #[cfg(feature = "std")]
    impl <T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                contract_id: None
            }
        }
    }

    #[pallet::genesis_build]
    impl <T: Config> GenesisBuild<T> for GenesisConfig<T> {
        fn build(&self) {
            if let Some(contract) = &self.contract_id {
                <ContractId<T>>::put(contract.clone());
            }

        }
    }
    
    #[pallet::pallet]
    #[pallet::generate_store(pub(super) trait Store)]
    pub struct Pallet<T>(_);


    #[pallet::storage]
    #[pallet::getter(fn lp_balances)]
    pub type LpBalances<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId,T::Balance, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn total_supply)]
    pub type TotalSupply<T: Config> = StorageValue<_,T::Balance,ValueQuery>;

    pub fn asset_1<T: Config>() -> Vec<u8> {
        T::Asset1::get().code.to_vec()
    }

    pub fn asset_2<T: Config>() -> Vec<u8> {
        T::Asset2::get().code.to_vec()
    }

    pub fn issuer_1<T: Config>() -> Vec<u8> {
        T::Asset1::get().issuer.to_vec()
    }

    pub fn issuer_2<T: Config>() -> Vec<u8> {
        T::Asset2::get().issuer.to_vec()
    }

    #[derive(Debug,Clone, Encode, Decode, Eq, PartialEq, Default, MaxEncodedLen)]
    #[cfg_attr(feature = "std", derive(Serialize, Deserialize, scale_info::TypeInfo))]
    pub(super) struct _Reserves<Balance> {
        reserve_0:Balance,
        reserve_1:Balance
    }

    #[pallet::type_value]
    pub(super) fn ReservesDefault<T: Config>() -> _Reserves<T::Balance> {
        _Reserves {
            reserve_0: T::Balance::default(),
            reserve_1: T::Balance::default()
        }
    }

    #[pallet::storage]
    pub(super) type Reserves<T: Config> = StorageValue<_,_Reserves<T::Balance>,ValueQuery,ReservesDefault<T>>;

    pub fn reserves<T: Config>() -> (T::Balance,T::Balance) {
        let res = <Reserves<T>>::get();

        (res.reserve_0, res.reserve_1)
    }


    #[pallet::storage]
    pub(super) type ContractId<T: Config> = StorageValue<_,T::AccountId,OptionQuery>;

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
            amount_to_send: T::Balance,
            amount_to_receive: T::Balance
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


    fn swap<T: Config>(
        contract: &T::AccountId,
        from: &T::AccountId,
        amount_to_receive: T::Balance,
        asset_to_receive: Asset
    ) -> DispatchResult {
        ensure!(
            amount_to_receive > T::Balance::zero(),
            Error::<T>::InsufficientOutputAmount
        );

        let asset_0 = T::Asset1::get();
        let asset_1 = T::Asset2::get();

        let reserves = <Reserves<T>>::get();


        if (asset_to_receive == asset_0 && amount_to_receive > reserves.reserve_0) ||
            (asset_to_receive == asset_1 && amount_to_receive > reserves.reserve_1) {
            return Err(DispatchError::from(Error::<T>::InsufficientLiquidity))
        }

        let balance_0 = balance_of::<T>(contract, asset_0.clone());
        let balance_1 = balance_of::<T>(contract, asset_1.clone());

        let (amount_to_send, asset_to_send) = if asset_to_receive == asset_0 {
            (amount_to_receive * balance_1 / (balance_0 - amount_to_receive), asset_1.clone())
        } else {
            (amount_to_receive * balance_0 / (balance_1 - amount_to_receive), asset_0.clone())
        };

        transfer_tokens::<T>(from,contract, asset_to_send,amount_to_send)?;
        transfer_tokens::<T>(contract,from, asset_to_receive, amount_to_receive)?;


        let balance_0 = balance_of::<T>(contract, asset_0);
        let balance_1 = balance_of::<T>(contract, asset_1);

        update::<T>(balance_0, balance_1);
        Ok(())
    }

    fn transfer_tokens<T: Config>(
        from: &T::AccountId,
        to: &T::AccountId,
        asset: Asset,
        amount: T::Balance
    ) -> DispatchResult {
        let from_balance = balance_of::<T>(from,asset.clone());
        ensure!(
            from_balance >= amount,
            Error::<T>::InsufficientBalance
        );

        T::BalanceExtension::transfer_balance(from,to,asset,amount);

        Ok(())
    }

    fn balance_of<T: Config>(owner:&T::AccountId, asset:Asset) -> T::Balance {
        T::BalanceExtension::fetch_balance(owner,asset)
    }

    fn update<T: Config>(balance_0: T::Balance, balance_1: T::Balance) {
        let reserves = _Reserves {
            reserve_0: balance_0,
            reserve_1: balance_1
        };

        <Reserves<T>>::put(reserves);
    }

    fn mint<T: Config>(to: &T::AccountId, value: T::Balance) -> Result<(),Error<T>>  {
        <TotalSupply<T>>::mutate(|v| { *v += value; });

        <LpBalances<T>>::get(to).map(|balance| {
            <LpBalances<T>>::insert(to.clone(), balance + value);
        })
        .ok_or(Error::<T>::ExtraError)
    }

    fn burn<T: Config>(from: &T::AccountId, value: T::Balance) -> DispatchResult {
        <TotalSupply<T>>::mutate(|v| { *v -= value; });

        <LpBalances<T>>::get(from).map(|balance| {
            <LpBalances<T>>::insert(from.clone(), balance - value);
        })
        .ok_or(DispatchError::from(Error::<T>::ExtraError))
    }

    fn calculate_liquidity<T:Config>(amount_0: T::Balance, amount_1: T::Balance, address_zero: &T::AccountId)
    -> Result<T::Balance,Error<T>> {
        let total_supply = <TotalSupply<T>>::get();
        let zero = T::Balance::zero();

        let liquidity: T::Balance = if total_supply == zero {
            let amount = amount_0 * amount_1;
            let min_liquidity = T::MinimumLiquidity::get();
            mint(address_zero, min_liquidity)?;
            amount.integer_sqrt() - min_liquidity
        }
        else {
            let reserves = <Reserves<T>>::get();
            cmp::min(
                amount_0 * total_supply / reserves.reserve_0,
                amount_1 * total_supply / reserves.reserve_1
            )
        };

        if liquidity <= zero {
           return Err(Error::<T>::InsufficientLiquidityMinted);
        };


        Ok(liquidity)
    }

    fn deposit<T: Config>(
        amount: T::Balance,
        asset: Asset,
        to: &T::AccountId
    ) -> Result<T::Balance,Error<T>> {

        let asset_0 = T::Asset1::get();
        let asset_1 = T::Asset2::get();

        let reserves = <Reserves<T>>::get();

        let contract = <ContractId<T>>::get().unwrap();

        let balance_0 = balance_of::<T>(&contract,asset_0.clone());
        let balance_1 = balance_of::<T>(&contract,asset_1.clone());

        let (amount_0, amount_1) = if asset == asset_0 {
            let amount_0 = amount;
            let amount_1 = if balance_0 > T::Balance::zero() { amount * balance_1 / balance_0 } else { amount };

            (amount_0, amount_1)
        } else {
            let amount_0 = if balance_1 > T::Balance::zero() { amount * balance_0 / balance_1 } else { amount };
            let amount_1 = amount;

            (amount_0, amount_1)
        };

        let user_balance_0 = balance_of::<T>(to,asset_0);
        let user_balance_1 = balance_of::<T>(to,asset_1);

        if amount_0 > user_balance_0 {
            return Err(Error::InsufficientBalance0);
        }

        if amount_1 > user_balance_1 {
            return Err(Error::InsufficientBalance1);
        }

        Ok(T::Balance::default())
    }

    fn _withdraw<T: Config>(amount: T::Balance, to: &T::AccountId) -> DispatchResult {
        let zero = T::Balance::zero();

        let total_supply = <TotalSupply<T>>::get();

        ensure!(
            total_supply != zero,
            Error::<T>::WithdrawWithoutSupply
        );

        if let Some(user_lp_balance) = <LpBalances<T>>::get(to) {
            ensure!(
                user_lp_balance >= amount,
                Error::<T>::InsufficientLiquidityBalance
            );

            let contract = <ContractId<T>>::get().unwrap();

            let asset_0 = T::Asset1::get();
            let asset_1 = T::Asset2::get();

            let balance_0 = balance_of::<T>(&contract, asset_0.clone());
            let balance_1 = balance_of::<T>(&contract, asset_1.clone());

            let amount_0 = amount * balance_0 / ((total_supply - amount) + amount);
            let amount_1 = amount * balance_1 / ((total_supply - amount) + amount);

            ensure!(
                (amount_0 > zero || amount_1 > zero),
                Error::<T>::InsufficientLiquidityBurned
            );

            transfer_tokens::<T>(&contract,to,asset_0.clone(), amount_0)?;
            transfer_tokens::<T>(&contract,to,asset_1.clone(),amount_1)?;
            burn::<T>(to,amount)?;

            let balance_0 = balance_of::<T>(&contract, asset_0);
            let balance_1 = balance_of::<T>(&contract, asset_1);
            update::<T>(balance_0,balance_1);

            return Ok(());

        }

        Err(DispatchError::from(Error::<T>::ExtraError))
    }

    // Dispatchable functions allows users to interact with the pallet and invoke state changes.
    // These functions materialize as "extrinsics", which are often compared to transactions.
    // Dispatchable functions must be annotated with a weight and must return a DispatchResult.
    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn deposit_asset_1(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let asset1 = T::Asset1::get();
            deposit::<T>(amount,asset1, &who)?;

            // Return a successful DispatchResultWithPostInfo
            Ok(())
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn deposit_asset_2(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;

            let asset2 = T::Asset2::get();
            deposit::<T>(amount,asset2, &who)?;

            // Return a successful DispatchResultWithPostInfo
            Ok(())
        }

        #[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
        pub fn withdraw(origin: OriginFor<T>, amount: T::Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            _withdraw::<T>(amount,&who)?;

            Ok(())

        }
    }
}

pub trait AmmExtended<AccountId, Balance> {
    fn fetch_balance(owner: &AccountId, asset: Asset) -> Balance;
    fn transfer_balance(from: &AccountId, to: &AccountId, asset: Asset, amount: Balance);
}

pub struct AmmExtendedEmpty<AccountId,Balance>(PhantomData<(AccountId, Balance)>);

impl <AccountId, Balance> AmmExtended<AccountId,Balance> for AmmExtendedEmpty<AccountId, Balance>
    where Balance: Zero {

    fn fetch_balance(_owner: &AccountId, _asset: Asset) -> Balance {
        Balance::zero()
    }

    fn transfer_balance(_from: &AccountId, _to: &AccountId,_asset: Asset, _amount: Balance) {
    }
}