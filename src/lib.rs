#![cfg_attr(not(feature = "std"), no_std)]

use ink_env::Environment;
use ink_lang as ink;

// type TokenId = [u8; 4];

#[derive(Copy, Clone)]
pub enum TokenId {
    EUR,
    USDC,
}

impl Into<[u8; 4]> for TokenId {
    fn into(self) -> [u8; 4] {
        match self {
            TokenId::EUR => [b'E', b'U', b'R', 0],
            TokenId::USDC => [b'U', b'S', b'D', b'C'],
        }
    }
}

#[ink::chain_extension]
pub trait BalanceExtension {
    type ErrorCode = BalanceReadErr;

    #[ink(extension = 1101, returns_result = false)]
    fn fetch_balance(owner: ink_env::AccountId, token: [u8; 4]) -> u128;

    #[ink(extension = 1102, returns_result = false, handle_status = false)]
    fn transfer_balance(
        from: ink_env::AccountId,
        to: ink_env::AccountId,
        token: [u8; 4],
        amount: u128,
    ) -> ();
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum BalanceReadErr {
    FailGetBalance,
}

impl ink_env::chain_extension::FromStatusCode for BalanceReadErr {
    fn from_status_code(status_code: u32) -> Result<(), Self> {
        match status_code {
            0 => Ok(()),
            1 => Err(Self::FailGetBalance),
            _ => panic!("encountered unknown status code"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum CustomEnvironment {}

impl Environment for CustomEnvironment {
    const MAX_EVENT_TOPICS: usize = <ink_env::DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;

    type AccountId = <ink_env::DefaultEnvironment as Environment>::AccountId;
    type Balance = <ink_env::DefaultEnvironment as Environment>::Balance;
    type Hash = <ink_env::DefaultEnvironment as Environment>::Hash;
    type BlockNumber = <ink_env::DefaultEnvironment as Environment>::BlockNumber;
    type Timestamp = <ink_env::DefaultEnvironment as Environment>::Timestamp;
    type RentFraction = <ink_env::DefaultEnvironment as Environment>::RentFraction;

    type ChainExtension = BalanceExtension;
}

#[ink::contract(env = crate::CustomEnvironment)]
pub mod amm {
    use crate::TokenId;

    #[cfg(not(feature = "ink-as-dependency"))]
    #[allow(unused_imports)]
    use ink_prelude::string::String;

    #[cfg(not(feature = "ink-as-dependency"))]
    use ink_storage::collections::HashMap as StorageHashMap;

    use num_integer::sqrt;

    /// The ERC-20 error types.
    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
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

    /// The ERC-20 result type.
    pub type Result<T> = core::result::Result<T, Error>;

    //type TokenId = [u8; 4];
    const MINIMUM_LIQUIDITY: u128 = 1;
    const ACCURACY_MULTIPLIER: u128 = 1_000;

    /// Event emitted when a token transfer occurs.
    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        from: Option<AccountId>,
        #[ink(topic)]
        to: Option<AccountId>,
        #[ink(topic)]
        value: Balance,
    }

    #[ink(event)]
    pub struct Mint {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        amount_usdc: Balance,
        #[ink(topic)]
        amount_eur: Balance,
    }

    #[ink(event)]
    pub struct Burn {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        to: AccountId,
        amount_usdc: Balance,
        amount_eur: Balance,
    }

    #[ink(event)]
    pub struct Swap {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        to: AccountId,
        amount_to_send: Balance,
        amount_to_receive: Balance,
    }

    #[ink(event)]
    pub struct Sync {
        #[ink(topic)]
        reserve_usdc: Balance,
        #[ink(topic)]
        reserve_eur: Balance,
    }

    #[ink(storage)]
    pub struct Pair {
        reserve_usdc: Balance,
        reserve_eur: Balance,

        total_supply: Balance,
        /// Mapping from owner to number of owned token.
        lp_balances: StorageHashMap<AccountId, Balance>,
    }

    impl Pair {
        #[ink(constructor)]
        pub fn new() -> Self {
            let caller = Self::env().caller();

            let instance = Self {
                total_supply: 0,
                lp_balances: Default::default(),
                reserve_usdc: 0,
                reserve_eur: 0,
            };

            Self::env().emit_event(Transfer {
                from: None,
                to: Some(caller),
                value: 0,
            });
            instance
        }

        /// Returns the total token supply.
        #[ink(message)]
        pub fn total_supply(&self) -> Balance {
            self.total_supply
        }

        /// Returns the account balance for the specified `owner`.
        ///
        /// Returns `0` if the account is non-existent.
        #[ink(message)]
        pub fn lp_balance_of(&self, owner: AccountId) -> Balance {
            *self.lp_balances.get(&owner).unwrap_or(&0)
        }

        pub fn balance_of(&self, owner: AccountId, token: TokenId) -> Balance {
            let balance = match self.env().extension().fetch_balance(owner, token.into()) {
                Ok(balance) => balance,
                // Err(err) => Err(BalanceReadErr::FailGetBalance),
                Err(_) => 0,
            };
            return balance;
        }

        #[ink(message)]
        pub fn minimum_liquidity(&self) -> u128 {
            return MINIMUM_LIQUIDITY;
        }

        #[ink(message)]
        pub fn get_reserves(&self) -> (Balance, Balance) {
            return (self.reserve_usdc, self.reserve_eur);
        }

        #[ink(message)]
        pub fn deposit_usdc(&mut self, amount_usdc: Balance) -> Result<Balance> {
            self.deposit(amount_usdc, TokenId::USDC)
        }

        #[ink(message)]
        pub fn deposit_eur(&mut self, amount_eur: Balance) -> Result<Balance> {
            self.deposit(amount_eur, TokenId::EUR)
        }

        pub fn deposit(&mut self, amount: Balance, token: TokenId) -> Result<Balance> {
            let contract = self.env().account_id();
            let from = self.env().caller();

            let (reserve_usdc, reserve_eur) = self.get_reserves();

            let balance_usdc = self.balance_of(contract, TokenId::USDC);
            let balance_eur = self.balance_of(contract, TokenId::EUR);

            let (amount_usdc, amount_eur) = match token {
                TokenId::USDC => (
                    amount,
                    if balance_usdc > 0 {
                        amount * balance_eur / balance_usdc
                    } else {
                        amount
                    },
                ),
                TokenId::EUR => (
                    if balance_eur > 0 {
                        amount * balance_usdc / balance_eur
                    } else {
                        amount
                    },
                    amount,
                ),
            };

            let user_balance_usdc = self.balance_of(from, TokenId::USDC);
            let user_balance_eur = self.balance_of(from, TokenId::EUR);
            if amount_usdc > user_balance_usdc {
                return Err(Error::InsufficientBalance0);
            }
            if amount_eur > user_balance_eur {
                return Err(Error::InsufficientBalance1);
            }

            let total_supply = self.total_supply;
            let liquidity: Balance;
            if total_supply == 0 {
                liquidity = sqrt(amount_usdc * amount_eur) - MINIMUM_LIQUIDITY;
                let address_zero = AccountId::from([0x01; 32]);
                self._mint(address_zero, MINIMUM_LIQUIDITY)?; // permanently lock first liquidity tokens
            } else {
                // upscale liquidity with ACCURACY_MULTIPLIER to improve precision
                // because usage of fractional numbers is not possible
                liquidity = core::cmp::min(
                    amount_usdc * ACCURACY_MULTIPLIER * total_supply / reserve_usdc,
                    amount_eur * ACCURACY_MULTIPLIER * total_supply / reserve_eur,
                );
            }

            if liquidity <= 0 {
                return Err(Error::InsufficientLiquidityMinted);
            }

            self.transfer_tokens(from, contract, TokenId::USDC, amount_usdc)?;
            self.transfer_tokens(from, contract, TokenId::EUR, amount_eur)?;
            self._mint(from, liquidity)?;

            let balance_usdc = self.balance_of(contract, TokenId::USDC);
            let balance_eur = self.balance_of(contract, TokenId::EUR);
            self._update(balance_usdc, balance_eur, reserve_usdc, reserve_eur)?;

            self.env().emit_event(Mint {
                sender: self.env().caller(),
                amount_usdc,
                amount_eur,
            });

            Ok(liquidity)
        }

        #[ink(message)]
        pub fn withdraw(&mut self, amount: Balance, to: AccountId) -> Result<(Balance, Balance)> {
            let total_supply = self.total_supply;
            if total_supply == 0 {
                return Err(Error::WithdrawWithoutSupply);
            }

            let user_lp_balance = self.lp_balance_of(to);
            if user_lp_balance < amount {
                return Err(Error::InsufficientLiquidityBalance);
            }

            let contract = self.env().account_id();
            let (reserve_usdc, reserve_eur) = self.get_reserves();
            let balance_usdc = self.balance_of(contract, TokenId::USDC);
            let balance_eur = self.balance_of(contract, TokenId::EUR);

            // rescale amounts with ACCURACY_MULTIPLIER to return proper amounts
            let amount_usdc = amount * balance_usdc
                / (((total_supply - amount) + amount / ACCURACY_MULTIPLIER) * ACCURACY_MULTIPLIER);
            let amount_eur = amount * balance_eur
                / (((total_supply - amount) + amount / ACCURACY_MULTIPLIER) * ACCURACY_MULTIPLIER);

            if !(amount_usdc > 0 || amount_eur > 0) {
                return Err(Error::InsufficientLiquidityBurned);
            }

            self.transfer_tokens(contract, to, TokenId::USDC, amount_usdc)?;
            self.transfer_tokens(contract, to, TokenId::EUR, amount_eur)?;
            self._burn(to, amount)?;

            let balance_usdc = self.balance_of(contract, TokenId::USDC);
            let balance_eur = self.balance_of(contract, TokenId::EUR);
            self._update(balance_usdc, balance_eur, reserve_usdc, reserve_eur)?;

            self.env().emit_event(Burn {
                sender: self.env().caller(),
                amount_usdc,
                amount_eur,
                to,
            });
            Ok((amount_usdc, amount_eur))
        }

        #[ink(message)]
        pub fn swap_usdc_to_eur(&mut self, amount_to_receive: Balance) -> Result<()> {
            self._swap(self.env().caller(), amount_to_receive, TokenId::EUR)
        }

        #[ink(message)]
        pub fn swap_eur_to_usdc(&mut self, amount_to_receive: Balance) -> Result<()> {
            self._swap(self.env().caller(), amount_to_receive, TokenId::USDC)
        }

        fn _swap(
            &mut self,
            from: AccountId,
            amount_to_receive: Balance,
            token_to_receive: TokenId,
        ) -> Result<()> {
            if amount_to_receive <= 0 {
                return Err(Error::InsufficientOutputAmount);
            }

            let (reserve_usdc, reserve_eur) = self.get_reserves();
            if match token_to_receive {
                TokenId::USDC => amount_to_receive > reserve_usdc,
                TokenId::EUR => amount_to_receive > reserve_eur,
            } {
                return Err(Error::InsufficientLiquidity);
            }

            let contract = self.env().account_id();
            let balance_usdc = self.balance_of(contract, TokenId::USDC);
            let balance_eur = self.balance_of(contract, TokenId::EUR);

            let (amount_to_send, token_to_send) = match token_to_receive {
                TokenId::USDC => (
                    amount_to_receive * balance_eur / (balance_usdc - amount_to_receive),
                    TokenId::EUR,
                ),
                TokenId::EUR => (
                    amount_to_receive * balance_usdc / (balance_eur - amount_to_receive),
                    TokenId::USDC,
                ),
            };

            self.transfer_tokens(from, contract, token_to_send, amount_to_send)?;
            self.transfer_tokens(contract, from, token_to_receive, amount_to_receive)?;

            let balance_usdc = self.balance_of(contract, TokenId::USDC);
            let balance_eur = self.balance_of(contract, TokenId::EUR);

            self._update(balance_usdc, balance_eur, reserve_usdc, reserve_eur)?;
            self.env().emit_event(Swap {
                sender: self.env().caller(),
                to: from,
                amount_to_send,
                amount_to_receive,
            });
            Ok(())
        }

        pub fn transfer_tokens(
            &mut self,
            from: AccountId,
            to: AccountId,
            token: TokenId,
            amount: Balance,
        ) -> Result<()> {
            let from_balance = self.balance_of(from, token);
            if from_balance < amount {
                return Err(Error::InsufficientBalance);
            }

            self.env()
                .extension()
                .transfer_balance(from, to, token.into(), amount);
            Ok(())
        }

        fn _update(
            &mut self,
            balance_usdc: Balance,
            balance_eur: Balance,
            reserve_usdc: Balance,
            reserve_eur: Balance,
        ) -> Result<()> {
            self.reserve_usdc = balance_usdc;
            self.reserve_eur = balance_eur;
            self.env().emit_event(Sync {
                reserve_usdc,
                reserve_eur,
            });
            Ok(())
        }

        fn _mint(&mut self, to: AccountId, value: Balance) -> Result<()> {
            self.total_supply += value;
            let balance = self.lp_balance_of(to);
            self.lp_balances.insert(to, balance + value);
            self.env().emit_event(Transfer {
                from: None,
                to: Some(to),
                value,
            });
            Ok(())
        }

        fn _burn(&mut self, from: AccountId, value: Balance) -> Result<()> {
            self.total_supply -= value;
            let balance = self.lp_balance_of(from);
            self.lp_balances.insert(from, balance - value);
            self.env().emit_event(Transfer {
                from: Some(from),
                to: None,
                value,
            });
            Ok(())
        }
    }

    /// Unit tests.
    #[cfg(not(feature = "ink-experimental-engine"))]
    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;

        type Event = <Pair as ::ink_lang::BaseEvent>::Type;

        use ink_lang as ink;

        const TOKEN_0: TokenId = [0, 0, 0, 0];
        const TOKEN_1: TokenId = [1, 1, 1, 1];
        const LP_TOKEN: TokenId = [2, 2, 2, 2];
        struct MockedExtension;
        impl ink_env::test::ChainExtension for MockedExtension {
            /// The static function id of the chain extension.
            fn func_id(&self) -> u32 {
                1101
            }

            /// The chain extension is called with the given input.
            ///
            /// Returns an error code and may fill the `output` buffer with a
            /// SCALE encoded result. The error code is taken from the
            /// `ink_env::chain_extension::FromStatusCode` implementation for
            /// `RandomReadErr`.
            fn call(&mut self, _input: &[u8], output: &mut Vec<u8>) -> u32 {
                let ret: [u8; 32] = [0; 32];
                // let ret = 1;
                scale::Encode::encode_to(&ret, output);
                println!("input: {:?}, output: {:?}", _input, output);
                0 // 0 is error code
            }
        }

        /// The default constructor does its job.
        #[ink::test]
        fn new_works() {
            // Constructor works.
            let initial_supply = 1_000;
            let pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let contract_balance_0 = pair.reserve_usdc;
            let contract_balance_1 = pair.reserve_eur;
            assert_eq!(contract_balance_0, contract_balance_1);
            assert_eq!(initial_supply, contract_balance_0);
        }

        #[ink::test]
        fn balance_of_works() {
            ink_env::test::register_chain_extension(MockedExtension);

            let to = AccountId::from([0x01; 32]);
            let initial_supply = 1_000;
            let pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);
            println!("balance of: balance: {}", pair.balance_of(to, TOKEN_0));
            assert_eq!(pair.balance_of(to, TOKEN_0), 0);
        }

        #[ink::test]
        fn deposit_works_for_balanced_pair() {
            ink_env::test::register_chain_extension(MockedExtension);
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let deposit_amount = 100;

            let user_balance_0_pre_deposit = pair.balance_of(to, TOKEN_0);
            let user_balance_1_pre_deposit = pair.balance_of(to, TOKEN_1);

            let result = pair.deposit(deposit_amount, TOKEN_0, to);
            let gained_lp = result.expect("Could not unwrap gained lp");
            assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

            let user_balance_0_post_deposit = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_deposit = pair.balance_of(to, TOKEN_1);

            let amount_0_in = user_balance_0_pre_deposit - user_balance_0_post_deposit;
            let amount_1_in = user_balance_1_pre_deposit - user_balance_1_post_deposit;
            // both balances should decrease equally because the asset pair is 1:1
            // i.e. the user has to pay an equal amount of each token
            assert_eq!(amount_0_in, amount_1_in);
            assert_eq!(
                user_balance_0_pre_deposit - deposit_amount,
                user_balance_0_post_deposit
            );
            assert_eq!(
                user_balance_1_pre_deposit - deposit_amount,
                user_balance_1_post_deposit
            );

            // check contract balances
            let contract_balance_0_post_deposit = pair.reserve_usdc;
            let contract_balance_1_post_deposit = pair.reserve_eur;
            assert_eq!(
                contract_balance_0_post_deposit,
                contract_balance_1_post_deposit
            );
            assert_eq!(
                initial_supply + deposit_amount,
                contract_balance_0_post_deposit
            );
        }

        #[ink::test]
        fn deposit_works_for_unbalanced_pair() {
            ink_env::test::register_chain_extension(MockedExtension);
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            pair.swap(TOKEN_0, 100, to).expect("Swap did not work");

            let deposit_amount = 100;
            let user_balance_0_pre_deposit = pair.balance_of(to, TOKEN_0);
            let user_balance_1_pre_deposit = pair.balance_of(to, TOKEN_1);

            let result = pair.deposit(deposit_amount, TOKEN_0, to);
            let gained_lp = result.expect("Could not unwrap gained lp");
            assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

            let user_balance_0_post_deposit = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_deposit = pair.balance_of(to, TOKEN_1);

            let amount_0_in = user_balance_0_pre_deposit - user_balance_0_post_deposit;
            let amount_1_in = user_balance_1_pre_deposit - user_balance_1_post_deposit;

            assert_eq!(deposit_amount, amount_0_in);
            // expect that amount_0_in is less than amount_1_in because
            // the pair has a ratio of 900:1111 after the swap thus TOKEN_0 is more valuable
            assert_eq!(true, amount_0_in < amount_1_in);
        }

        #[ink::test]
        fn withdraw_without_lp_fails() {
            ink_env::test::register_chain_extension(MockedExtension);
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let result = pair.withdraw(1, to);
            assert_eq!(Err(Error::WithdrawWithoutSupply), result);

            let gained_lp = pair.deposit(5_000, TOKEN_0, to).expect("Could not deposit");
            // try withdrawing more LP than account has
            let result = pair.withdraw(gained_lp + 2, to);
            assert_eq!(Err(Error::InsufficientLiquidityBalance), result);
        }

        #[ink::test]
        fn withdraw_works() {
            ink_env::test::register_chain_extension(MockedExtension);
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let deposit_amount = 5_000_00;
            let result = pair.deposit(deposit_amount, TOKEN_0, to);
            let gained_lp = result.expect("Could not unwrap gained lp");
            assert_eq!(
                gained_lp > 0,
                true,
                "Expected received amount of LP to be greater than 0"
            );

            let user_balance_0_pre_withdraw = pair.balance_of(to, TOKEN_0);
            let user_balance_1_pre_withdraw = pair.balance_of(to, TOKEN_1);

            let result = pair.withdraw(gained_lp, to);
            let (amount_usdc, amount_eur) = result.expect("Could not unwrap result");
            assert_eq!(
                true,
                amount_usdc > 0,
                "Expected received amount to be greater than 0"
            );
            assert_eq!(
                true,
                amount_eur > 0,
                "Expected received amount to be greater than 0"
            );

            let user_balance_0_post_withdraw = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_withdraw = pair.balance_of(to, TOKEN_1);

            assert_eq!(
                user_balance_0_post_withdraw,
                user_balance_0_pre_withdraw + amount_usdc
            );
            assert_eq!(
                user_balance_1_post_withdraw,
                user_balance_1_pre_withdraw + amount_eur
            );
        }

        #[ink::test]
        fn deposit_and_withdraw_work() {
            ink_env::test::register_chain_extension(MockedExtension);
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let deposit_amount = 5_000_00;
            // do initial deposit which initiates total_supply
            pair.deposit(deposit_amount, TOKEN_0, to)
                .expect("Could not deposit");

            // do second deposit
            let result = pair.deposit(deposit_amount, TOKEN_0, to);
            let gained_lp = result.expect("Could not unwrap gained lp");

            assert_eq!(
                gained_lp > 0,
                true,
                "Expected received amount of LP to be greater than 0"
            );

            let result = pair.withdraw(gained_lp, to);
            let (amount_usdc, amount_eur) = result.expect("Could not unwrap result");

            let user_balance_0_post_withdraw = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_withdraw = pair.balance_of(to, TOKEN_1);
            assert_eq!(
                amount_usdc, deposit_amount,
                "Expected withdrawn amount to be equal to deposited amount"
            );
            assert_eq!(
                amount_eur, deposit_amount,
                "Expected withdrawn amount to be equal to deposited amount"
            );
        }

        #[ink::test]
        fn swap_works_with_small_amount() {
            ink_env::test::register_chain_extension(MockedExtension);
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let gained_lp = pair.deposit(5, TOKEN_0, to);
            let gained_lp = gained_lp.expect("Could not unwrap gained lp");
            assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

            let swap_amount = 100;
            let rate: f64 =
                pair.reserve_usdc as f64 / ((pair.reserve_eur as f64) - (swap_amount as f64));
            println!("Expected float conversion rate: {}", rate);
            let user_balance_0_pre_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_pre_swap = pair.balance_of(to, TOKEN_1);
            println!(
                "Balances pre swap: {}, {}",
                user_balance_0_pre_swap, user_balance_1_pre_swap
            );

            let result = pair.swap(TOKEN_0, swap_amount, to);
            result.expect("Encountered error in swap");
            let user_balance_0_post_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_swap = pair.balance_of(to, TOKEN_1);
            assert_eq!(
                user_balance_0_post_swap,
                user_balance_0_pre_swap + swap_amount
            );
            assert_eq!(
                user_balance_1_post_swap,
                ((user_balance_1_pre_swap as f64) - (swap_amount as f64) * rate).round() as u128
            );

            let rate: f64 =
                pair.reserve_eur as f64 / ((pair.reserve_usdc as f64) - (swap_amount as f64));
            println!("Expected exact float conversion rate: {}", rate);
            let user_balance_0_pre_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_pre_swap = pair.balance_of(to, TOKEN_1);
            println!(
                "Balances pre swap: {}, {}",
                user_balance_0_pre_swap, user_balance_1_pre_swap
            );

            let result = pair.swap(TOKEN_1, swap_amount, to);
            result.expect("Encountered error in swap");

            let user_balance_0_post_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_swap = pair.balance_of(to, TOKEN_1);
            println!(
                "Balances post swap: {}, {}",
                user_balance_0_post_swap, user_balance_1_post_swap
            );
            assert_eq!(
                user_balance_0_post_swap,
                ((user_balance_0_pre_swap as f64) - (swap_amount as f64) * rate).round() as u128
            );
            assert_eq!(
                user_balance_1_post_swap,
                user_balance_1_pre_swap + swap_amount
            );
        }

        #[ink::test]
        fn swap_works_with_large_amount() {
            ink_env::test::register_chain_extension(MockedExtension);
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let gained_lp = pair.deposit(5, TOKEN_0, to);
            let gained_lp = gained_lp.expect("Could not unwrap gained lp");
            assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

            let swap_amount = 200_000;
            let rate: f64 =
                pair.reserve_usdc as f64 / ((pair.reserve_eur as f64) - (swap_amount as f64));
            println!("Expected float conversion rate: {}", rate);
            let user_balance_0_pre_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_pre_swap = pair.balance_of(to, TOKEN_1);
            println!(
                "Balances pre swap: {}, {}",
                user_balance_0_pre_swap, user_balance_1_pre_swap
            );

            let result = pair.swap(TOKEN_0, swap_amount, to);
            result.expect("Encountered error in swap");
            let user_balance_0_post_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_swap = pair.balance_of(to, TOKEN_1);
            assert_eq!(
                user_balance_0_post_swap,
                user_balance_0_pre_swap + swap_amount
            );
            println!(
                "Expected without round {}",
                ((user_balance_1_pre_swap as f64) - (swap_amount as f64) * rate)
            );
            assert_eq!(
                user_balance_1_post_swap,
                ((user_balance_1_pre_swap as f64) - (swap_amount as f64) * rate).round() as u128
            );

            let rate: f64 =
                pair.reserve_eur as f64 / ((pair.reserve_usdc as f64) - (swap_amount as f64));
            println!("Expected exact float conversion rate: {}", rate);
            let user_balance_0_pre_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_pre_swap = pair.balance_of(to, TOKEN_1);
            println!(
                "Balances pre swap: {}, {}",
                user_balance_0_pre_swap, user_balance_1_pre_swap
            );
            let result = pair.swap(TOKEN_1, swap_amount, to);
            result.expect("Encountered error in swap");
            let user_balance_0_post_swap = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_swap = pair.balance_of(to, TOKEN_1);
            assert_eq!(
                user_balance_0_post_swap,
                ((user_balance_0_pre_swap as f64) - (swap_amount as f64) * rate).round() as u128
            );
            assert_eq!(
                user_balance_1_post_swap,
                user_balance_1_pre_swap + swap_amount
            );
        }
    }
}
