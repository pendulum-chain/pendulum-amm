#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

#[ink::contract]
mod amm {
    #[cfg(not(feature = "ink-as-dependency"))]
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

    type TokenId = [u8; 4];
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
        amount_0: Balance,
        #[ink(topic)]
        amount_1: Balance,
    }

    #[ink(event)]
    pub struct Burn {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        to: AccountId,
        amount_0: Balance,
        amount_1: Balance,
    }

    #[ink(event)]
    pub struct Swap {
        #[ink(topic)]
        sender: AccountId,
        #[ink(topic)]
        to: AccountId,
        amount_0_in: Balance,
        amount_1_in: Balance,
        amount_0_out: Balance,
        amount_1_out: Balance,
    }

    #[ink(event)]
    pub struct Sync {
        #[ink(topic)]
        reserve_0: Balance,
        #[ink(topic)]
        reserve_1: Balance,
    }

    #[ink(storage)]
    pub struct Pair {
        token_0: TokenId,
        token_1: TokenId,
        lp_token: TokenId,

        reserve_0: Balance,
        reserve_1: Balance,
        block_timestamp_last: Timestamp,

        price_0_cumulative_last: Balance,
        price_1_cumulative_last: Balance,
        k_last: Balance, // reserve0 * reserve1, as of immediately after the most recent liquidity event

        unlocked: u32,

        fee_to: Option<AccountId>, // address of account that receives fee

        total_supply: Balance,
        /// Mapping from owner to number of owned token.
        balances: StorageHashMap<(AccountId, TokenId), Balance>,
    }

    impl Pair {
        #[ink(constructor)]
        pub fn new(
            token_0: TokenId,
            initial_supply_0: Balance,
            token_1: TokenId,
            initial_supply_1: Balance,
            lp_token: TokenId,
        ) -> Self {
            let caller = Self::env().caller();
            let contract = Self::env().account_id();
            let mut balances = StorageHashMap::new();
            balances.insert((caller, token_0), initial_supply_0);
            balances.insert((caller, token_1), initial_supply_1);
            balances.insert((contract, token_0), initial_supply_0);
            balances.insert((contract, token_1), initial_supply_1);

            let instance = Self {
                total_supply: 0,
                balances,
                token_0,
                token_1,
                lp_token,
                reserve_0: initial_supply_0,
                reserve_1: initial_supply_1,
                block_timestamp_last: 0,
                price_0_cumulative_last: 0,
                price_1_cumulative_last: 0,
                k_last: 0,
                unlocked: 1,
                fee_to: None,
            };
            Self::env().emit_event(Transfer {
                from: None,
                to: Some(caller),
                value: initial_supply_0 + initial_supply_1,
            });
            instance
        }

        #[ink(message)]
        pub fn token_0(&self) -> String {
            return String::from_utf8(self.token_0.to_vec()).unwrap();
        }

        #[ink(message)]
        pub fn token_1(&self) -> String {
            return String::from_utf8(self.token_1.to_vec()).unwrap();
        }

        #[ink(message)]
        pub fn lp_token(&self) -> String {
            return String::from_utf8(self.lp_token.to_vec()).unwrap();
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
        pub fn balance_of(&self, owner: AccountId, token: TokenId) -> Balance {
            self.balances.get(&(owner, token)).copied().unwrap_or(0)
        }

        /// Transfers `value` amount of tokens from the caller's account to account `to`.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the caller's account balance.
        // #[ink(message)]
        pub fn transfer(&mut self, to: AccountId, token: TokenId, value: Balance) -> Result<()> {
            let from = self.env().caller();
            self.transfer_from_to(from, to, token, value)
        }

        /// Transfers `value` amount of tokens from the caller's account to account `to`.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the caller's account balance.
        fn transfer_from_to(
            &mut self,
            from: AccountId,
            to: AccountId,
            token: TokenId,
            value: Balance,
        ) -> Result<()> {
            let from_balance = self.balance_of(from, token);
            if from_balance < value {
                return Err(Error::InsufficientBalance);
            }
            self.balances.insert((from, token), from_balance - value);
            let to_balance = self.balance_of(to, token);
            self.balances.insert((to, token), to_balance + value);
            self.env().emit_event(Transfer {
                from: Some(from),
                to: Some(to),
                value,
            });
            Ok(())
        }

        #[ink(message)]
        pub fn minimum_liquidity(&self) -> u128 {
            return MINIMUM_LIQUIDITY;
        }

        #[ink(message)]
        pub fn get_reserves(&self) -> (Balance, Balance, Timestamp) {
            return (self.reserve_0, self.reserve_1, self.block_timestamp_last);
        }

        #[ink(message)]
        pub fn deposit(
            &mut self,
            amount: Balance,
            token: TokenId,
            from: AccountId,
        ) -> Result<Balance> {
            let contract = self.env().account_id();
            let token_0 = self.token_0;
            let token_1 = self.token_1;
            if token != token_0 && token != token_1 {
                return Err(Error::InvalidDepositToken);
            }

            let (reserve_0, reserve_1, _) = self.get_reserves();
            let balance_0 = self.balance_of(contract, token_0);
            let balance_1 = self.balance_of(contract, token_1);
            let amount_0 = if token == token_0 {
                amount
            } else {
                amount * balance_0 / balance_1
            };
            let amount_1 = if token == token_0 {
                amount * balance_1 / balance_0
            } else {
                amount
            };

            let user_balance_0 = self.balance_of(from, token_0);
            let user_balance_1 = self.balance_of(from, token_1);
            if amount_0 > user_balance_0 {
                return Err(Error::InsufficientBalance0);
            }
            if amount_1 > user_balance_1 {
                return Err(Error::InsufficientBalance1);
            }

            let fee_on = self._mint_fee(reserve_0, reserve_1);
            let total_supply = self.total_supply;
            let liquidity: Balance;
            if total_supply == 0 {
                liquidity = sqrt(amount_0 * amount_1) - MINIMUM_LIQUIDITY;
                let address_zero = AccountId::from([0x01; 32]);
                self._mint(address_zero, MINIMUM_LIQUIDITY)?; // permanently lock first liquidity tokens
            } else {
                // upscale liquidity with ACCURACY_MULTIPLIER to improve precision
                // because usage of fractional numbers is not possible
                liquidity = core::cmp::min(
                    amount_0 * ACCURACY_MULTIPLIER * total_supply / reserve_0,
                    amount_1 * ACCURACY_MULTIPLIER * total_supply / reserve_1,
                );
            }

            if liquidity <= 0 {
                return Err(Error::InsufficientLiquidityMinted);
            }

            self.transfer_from_to(from, contract, token_0, amount_0)?;
            self.transfer_from_to(from, contract, token_1, amount_1)?;
            self._mint(from, liquidity)?;

            let balance_0 = self.balance_of(contract, token_0);
            let balance_1 = self.balance_of(contract, token_1);
            self._update(balance_0, balance_1, reserve_0, reserve_1)?;
            if fee_on {
                self.k_last = reserve_0 * reserve_1;
            }

            self.env().emit_event(Mint {
                sender: self.env().caller(),
                amount_0,
                amount_1,
            });

            Ok(liquidity)
        }

        #[ink(message)]
        pub fn withdraw(&mut self, amount: Balance, to: AccountId) -> Result<(Balance, Balance)> {
            let total_supply = self.total_supply;
            if total_supply == 0 {
                return Err(Error::WithdrawWithoutSupply);
            }

            let user_lp_balance = self.balance_of(to, self.lp_token);
            if user_lp_balance < amount {
                return Err(Error::InsufficientLiquidityBalance);
            }

            let contract = self.env().account_id();
            let (reserve_0, reserve_1, _) = self.get_reserves();
            let token_0 = self.token_0;
            let token_1 = self.token_1;
            let balance_0 = self.balance_of(contract, token_0);
            let balance_1 = self.balance_of(contract, token_1);

            let fee_on = self._mint_fee(reserve_0, reserve_1);
            // rescale amounts with ACCURACY_MULTIPLIER to return proper amounts
            let amount_0 = amount * balance_0
                / (((total_supply - amount) + amount / ACCURACY_MULTIPLIER) * ACCURACY_MULTIPLIER);
            let amount_1 = amount * balance_1
                / (((total_supply - amount) + amount / ACCURACY_MULTIPLIER) * ACCURACY_MULTIPLIER);

            if !(amount_0 > 0 || amount_1 > 0) {
                return Err(Error::InsufficientLiquidityBurned);
            }
            self.transfer_from_to(contract, to, token_0, amount_0)?;
            self.transfer_from_to(contract, to, token_1, amount_1)?;
            self._burn(to, amount)?;

            let balance_0 = self.balance_of(contract, token_0);
            let balance_1 = self.balance_of(contract, token_1);

            self._update(balance_0, balance_1, reserve_0, reserve_1)?;
            if fee_on {
                self.k_last = reserve_0 * reserve_1;
            }
            self.env().emit_event(Burn {
                sender: self.env().caller(),
                amount_0,
                amount_1,
                to,
            });
            Ok((amount_0, amount_1))
        }

        #[ink(message)]
        pub fn swap(&mut self, token_to_receive: TokenId, amount: Balance, account: AccountId) -> Result<()> {
            if token_to_receive == self.token_0 {
                return self._swap(amount, 0, account);
            } else if token_to_receive == self.token_1 {
                return self._swap(0, amount, account);
            } else {
                return Err(Error::InvalidSwapToken);
            }
        }

        fn _swap(
            &mut self,
            amount_0_out: Balance,
            amount_1_out: Balance,
            to: AccountId,
        ) -> Result<()> {
            if !(amount_0_out > 0 || amount_1_out > 0) {
                return Err(Error::InsufficientOutputAmount);
            }
            let (reserve_0, reserve_1, _) = self.get_reserves();
            if amount_0_out > reserve_0 || amount_1_out > reserve_1 {
                return Err(Error::InsufficientLiquidity);
            }

            let token_0 = self.token_0;
            let token_1 = self.token_1;

            let contract = self.env().account_id();
            let balance_0 = self.balance_of(contract, token_0);
            let balance_1 = self.balance_of(contract, token_1);

            let amount_0_in = if balance_0 > reserve_0 - amount_0_out {
                balance_0 - (reserve_0 - amount_0_out)
            } else {
                0
            };
            let amount_1_in = if balance_1 > reserve_1 - amount_1_out {
                balance_1 - (reserve_1 - amount_1_out)
            } else {
                0
            };

            if !(amount_0_in > 0 || amount_1_in > 0) {
                return Err(Error::InsufficientInputAmount);
            }
            if amount_0_out > 0 {
                let converted_amount_in = amount_0_out * balance_0 / (balance_1 - amount_0_out);
                self.transfer_from_to(to, contract, token_1, converted_amount_in)?;
                self.transfer_from_to(contract, to, token_0, amount_0_out)?;
            }
            if amount_1_out > 0 {
                let converted_amount_in = amount_1_out * balance_1 / (balance_0 - amount_1_out);
                self.transfer_from_to(to, contract, token_0, converted_amount_in)?;
                self.transfer_from_to(contract, to, token_1, amount_1_out)?;
            }

            let balance_0 = self.balance_of(contract, token_0);
            let balance_1 = self.balance_of(contract, token_1);

            self._update(balance_0, balance_1, reserve_0, reserve_1)?;
            self.env().emit_event(Swap {
                sender: self.env().caller(),
                amount_0_in,
                amount_1_in,
                amount_0_out,
                amount_1_out,
                to,
            });
            Ok(())
        }

        /// force balances to match reserves
        #[ink(message)]
        pub fn skim(&mut self, to: AccountId) -> Result<()> {
            let contract = self.env().account_id();
            let token_0 = self.token_0;
            let token_1 = self.token_1;
            let balance_0 = self.balance_of(contract, token_0);
            let balance_1 = self.balance_of(contract, token_1);
            self.transfer_from_to(contract, to, token_0, balance_0 - self.reserve_0)?;
            self.transfer_from_to(contract, to, token_1, balance_1 - self.reserve_1)?;

            Ok(())
        }

        #[ink(message)]
        pub fn sync(&mut self) -> Result<()> {
            let contract = self.env().account_id();
            self._update(
                self.balance_of(contract, self.token_0),
                self.balance_of(contract, self.token_1),
                self.reserve_0,
                self.reserve_1,
            )?;
            Ok(())
        }

        fn _update(
            &mut self,
            balance_0: Balance,
            balance_1: Balance,
            reserve_0: Balance,
            reserve_1: Balance,
        ) -> Result<()> {
            let block_timestamp = self.env().block_timestamp();
            let time_elapsed: u128 = (block_timestamp - self.block_timestamp_last).into();

            if time_elapsed > 0 && reserve_0 != 0 && reserve_1 != 0 {
                self.price_0_cumulative_last += reserve_1 * time_elapsed / reserve_0;
                self.price_1_cumulative_last += reserve_0 * time_elapsed / reserve_1;
            }
            self.reserve_0 = balance_0;
            self.reserve_1 = balance_1;
            self.block_timestamp_last = block_timestamp;
            self.env().emit_event(Sync {
                reserve_0,
                reserve_1,
            });
            Ok(())
        }

        fn _mint_fee(&mut self, reserve_0: Balance, reserve_1: Balance) -> bool {
            let fee_to = self.fee_to;
            if let Some(account) = fee_to {
                if self.k_last != 0 {
                    let root_k = sqrt(reserve_0 * reserve_1);
                    let root_k_last = sqrt(self.k_last);
                    if root_k > root_k_last {
                        let numerator = self.total_supply * (root_k - root_k_last);
                        let denominator = root_k * 5 + root_k_last;
                        let liquidity = numerator / denominator;
                        if liquidity > 0 {
                            match self._mint(account, liquidity) {
                                Ok(_) => return true,
                                Err(_) => return false,
                            }
                        }
                    }
                }
                return true;
            } else {
                return false;
            }
        }

        fn _mint(&mut self, to: AccountId, value: Balance) -> Result<()> {
            self.total_supply += value;
            let balance = self.balance_of(to, self.lp_token);
            self.balances.insert((to, self.lp_token), balance + value);
            self.env().emit_event(Transfer {
                from: None,
                to: Some(to),
                value,
            });
            Ok(())
        }

        fn _burn(&mut self, from: AccountId, value: Balance) -> Result<()> {
            self.total_supply -= value;
            let balance = self.balance_of(from, self.lp_token);
            self.balances.insert((from, self.lp_token), balance - value);
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

        /// The default constructor does its job.
        #[ink::test]
        fn new_works() {
            // Constructor works.
            let initial_supply = 1_000;
            let pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let contract_balance_0 = pair.reserve_0;
            let contract_balance_1 = pair.reserve_1;
            assert_eq!(contract_balance_0, contract_balance_1);
            assert_eq!(initial_supply, contract_balance_0);
        }

        #[ink::test]
        fn deposit_works_for_balanced_pair() {
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
            let contract_balance_0_post_deposit = pair.reserve_0;
            let contract_balance_1_post_deposit = pair.reserve_1;
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
            let (amount_0, amount_1) = result.expect("Could not unwrap result");
            assert_eq!(
                true,
                amount_0 > 0,
                "Expected received amount to be greater than 0"
            );
            assert_eq!(
                true,
                amount_1 > 0,
                "Expected received amount to be greater than 0"
            );

            let user_balance_0_post_withdraw = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_withdraw = pair.balance_of(to, TOKEN_1);

            assert_eq!(
                user_balance_0_post_withdraw,
                user_balance_0_pre_withdraw + amount_0
            );
            assert_eq!(
                user_balance_1_post_withdraw,
                user_balance_1_pre_withdraw + amount_1
            );
        }

        #[ink::test]
        fn deposit_and_withdraw_work() {
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
            let (amount_0, amount_1) = result.expect("Could not unwrap result");

            let user_balance_0_post_withdraw = pair.balance_of(to, TOKEN_0);
            let user_balance_1_post_withdraw = pair.balance_of(to, TOKEN_1);
            assert_eq!(
                amount_0, deposit_amount,
                "Expected withdrawn amount to be equal to deposited amount"
            );
            assert_eq!(
                amount_1, deposit_amount,
                "Expected withdrawn amount to be equal to deposited amount"
            );
        }

        #[ink::test]
        fn swap_works_with_small_amount() {
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let gained_lp = pair.deposit(5, TOKEN_0, to);
            let gained_lp = gained_lp.expect("Could not unwrap gained lp");
            assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

            let swap_amount = 100;
            let rate: f64 =
                pair.reserve_0 as f64 / ((pair.reserve_1 as f64) - (swap_amount as f64));
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
                pair.reserve_1 as f64 / ((pair.reserve_0 as f64) - (swap_amount as f64));
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
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(TOKEN_0, initial_supply, TOKEN_1, initial_supply, LP_TOKEN);

            let gained_lp = pair.deposit(5, TOKEN_0, to);
            let gained_lp = gained_lp.expect("Could not unwrap gained lp");
            assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

            let swap_amount = 200_000;
            let rate: f64 =
                pair.reserve_0 as f64 / ((pair.reserve_1 as f64) - (swap_amount as f64));
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
                pair.reserve_1 as f64 / ((pair.reserve_0 as f64) - (swap_amount as f64));
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
