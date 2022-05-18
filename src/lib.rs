#![cfg_attr(not(feature = "std"), no_std)]

use ink_env::Environment;
use ink_lang as ink;

pub type AssetCode = [u8; 12];
pub type IssuerId = [u8; 32]; // encoded 32-bit array of 56 character stellar issuer (public key)
pub type Asset = (IssuerId, AssetCode);

#[ink::chain_extension]
pub trait BalanceExtension {
    type ErrorCode = BalanceReadErr;

    #[ink(extension = 1101, returns_result = false)]
    fn fetch_balance(owner: ink_env::AccountId, asset: Asset) -> u128;

    #[ink(extension = 1102, returns_result = false, handle_status = false)]
    fn transfer_balance(
        from: ink_env::AccountId,
        to: ink_env::AccountId,
        asset: Asset,
        amount: u128,
    ) -> ();
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, scale::Encode, scale::Decode)]
#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
pub enum BalanceReadErr {
    FailGetBalance,
    FailTransferBalance,
}

impl ink_env::chain_extension::FromStatusCode for BalanceReadErr {
    fn from_status_code(status_code: u32) -> Result<(), Self> {
        match status_code {
            0 => Ok(()),
            1 => Err(Self::FailGetBalance),
            1 => Err(Self::FailTransferBalance),
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

    type ChainExtension = BalanceExtension;
}

pub mod util {
    use crate::amm::Error;
    use crate::AssetCode;

    pub fn asset_from_string(str: ink_prelude::string::String) -> Result<AssetCode, Error> {
        let str: &[u8] = str.as_ref();
        if str.len() > 12 {
            return Err(Error::AssetCodeTooLong);
        }

        if !str.iter().all(|char| {
            let char = char::from(*char);
            char.is_ascii_alphanumeric()
        }) {
            return Err(Error::InvalidAssetCodeCharacter);
        }

        let mut asset_code_array: AssetCode = [0; 12];
        asset_code_array[..str.len()].copy_from_slice(str);
        Ok(asset_code_array)
    }

    pub fn trim_zeros(x: &[u8]) -> &[u8] {
        let from = match x.iter().position(|&x| x != 0) {
            Some(i) => i,
            None => return &x[0..0],
        };
        let to = x.iter().rposition(|&x| x != 0).unwrap();
        &x[from..=to]
    }
}

pub mod base32 {
    use core::convert::AsRef;

    #[cfg(not(feature = "ink-as-dependency"))]
    use ink_prelude::vec::Vec;

    use crate::amm::Error;

    const ALPHABET: &'static [u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    fn ascii_to_value_5bit(char: u8) -> Option<u8> {
        match char as char {
            'a'..='z' => Some(char - ('a' as u8)),
            'A'..='Z' => Some(char - ('A' as u8)),
            '2'..='7' => Some(char - ('2' as u8) + 26),
            '0' => Some(14),
            '1' => Some(8),
            _ => None,
        }
    }

    pub fn encode<T: AsRef<[u8]>>(binary: T) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(binary.as_ref().len() * 2);
        let mut shift = 3;
        let mut carry = 0;

        for byte in binary.as_ref().iter() {
            let value_5bit = if shift == 8 {
                carry
            } else {
                carry | ((*byte) >> shift)
            };
            buffer.push(ALPHABET[(value_5bit & 0x1f) as usize]);

            if shift > 5 {
                shift -= 5;
                let value_5bit = (*byte) >> shift;
                buffer.push(ALPHABET[(value_5bit & 0x1f) as usize]);
            }

            shift = 5 - shift;
            carry = *byte << shift;
            shift = 8 - shift;
        }

        if shift != 3 {
            buffer.push(ALPHABET[(carry & 0x1f) as usize]);
        }

        buffer
    }

    pub fn decode<T: AsRef<[u8]>>(string: T) -> Result<Vec<u8>, Error> {
        let mut result = Vec::with_capacity(string.as_ref().len());
        let mut shift: i8 = 8;
        let mut carry: u8 = 0;

        for ascii in string.as_ref().iter() {
            if *ascii as char == '=' {
                break;
            }

            let value_5bit = ascii_to_value_5bit(*ascii);
            if let Some(value_5bit) = value_5bit {
                shift -= 5;
                if shift > 0 {
                    carry |= value_5bit << shift;
                } else if shift < 0 {
                    result.push(carry | (value_5bit >> -shift));
                    shift += 8;
                    carry = value_5bit << shift;
                } else {
                    result.push(carry | value_5bit);
                    shift = 8;
                    carry = 0;
                }
            } else {
                return Err(Error::InvalidBase32Character);
            }
        }

        if shift != 8 && carry != 0 {
            result.push(carry);
        }

        Ok(result)
    }
}

pub mod key_encoding {
    use super::base32::{decode, encode};
    use core::convert::{AsRef, TryInto};

    #[cfg(not(feature = "ink-as-dependency"))]
    use ink_prelude::vec::Vec;

    use crate::amm::Error;

    pub const ED25519_PUBLIC_KEY_BYTE_LENGTH: usize = 32;
    pub const ED25519_PUBLIC_KEY_VERSION_BYTE: u8 = 6 << 3; // G

    pub const ED25519_SECRET_SEED_BYTE_LENGTH: usize = 32;
    pub const ED25519_SECRET_SEED_VERSION_BYTE: u8 = 18 << 3; // S

    pub const MED25519_PUBLIC_KEY_BYTE_LENGTH: usize = 40;
    pub const MED25519_PUBLIC_KEY_VERSION_BYTE: u8 = 12 << 3; // M

    /// Use Stellar's key encoding to decode a key given as an ASCII string (as `&[u8]`)
    pub fn decode_stellar_key<T: AsRef<[u8]>, const BYTE_LENGTH: usize>(
        encoded_key: T,
        version_byte: u8,
    ) -> Result<[u8; BYTE_LENGTH], Error> {
        let decoded_array = decode(encoded_key.as_ref())?;
        if *encoded_key.as_ref() != encode(&decoded_array)[..] {
            return Err(Error::InvalidStellarKeyEncoding);
        }

        let array_length = decoded_array.len();
        if array_length != 3 + BYTE_LENGTH {
            return Err(Error::InvalidStellarKeyEncodingLength);
        }

        let crc_value = ((decoded_array[array_length - 1] as u16) << 8)
            | decoded_array[array_length - 2] as u16;
        let expected_crc_value = crc(&decoded_array[..array_length - 2]);
        if crc_value != expected_crc_value {
            return Err(Error::InvalidStellarKeyChecksum {
                expected: expected_crc_value,
                found: crc_value,
            });
        }

        let expected_version = version_byte;
        if decoded_array[0] != expected_version {
            return Err(Error::InvalidStellarKeyEncodingVersion);
        }

        Ok(decoded_array[1..array_length - 2].try_into().unwrap())
    }

    /// Return the key encoding as an ASCII string (given as `Vec<u8>`)
    pub fn encode_stellar_key<const BYTE_LENGTH: usize>(
        key: &[u8; BYTE_LENGTH],
        version_byte: u8,
    ) -> Vec<u8> {
        let mut unencoded_array = Vec::with_capacity(3 + BYTE_LENGTH);
        unencoded_array.push(version_byte);
        unencoded_array.extend(key.iter());

        let crc_value = crc(&unencoded_array);
        unencoded_array.push((crc_value & 0xff) as u8);
        unencoded_array.push((crc_value >> 8) as u8);

        encode(&unencoded_array)
    }

    fn crc<T: AsRef<[u8]>>(byte_array: T) -> u16 {
        let mut crc: u16 = 0;

        for byte in byte_array.as_ref().iter() {
            let mut code: u16 = crc >> 8 & 0xff;

            code ^= *byte as u16;
            code ^= code >> 4;
            crc = (crc << 8) & 0xffff;
            crc ^= code;
            code = (code << 5) & 0xffff;
            crc ^= code;
            code = (code << 7) & 0xffff;
            crc ^= code;
        }

        crc
    }
}

#[ink::contract(env = crate::CustomEnvironment)]
pub mod amm {

    use crate::key_encoding::{
        decode_stellar_key, encode_stellar_key, ED25519_PUBLIC_KEY_BYTE_LENGTH,
        ED25519_PUBLIC_KEY_VERSION_BYTE,
    };
    use crate::util::{asset_from_string, trim_zeros};
    use crate::Asset;
    use ink_prelude::string::String;
    use ink_storage::traits::SpreadAllocate;
    use ink_storage::Mapping;
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

        // -- mod errors
        InvalidStellarKeyEncoding,
        InvalidStellarKeyEncodingLength,
        InvalidStellarKeyChecksum {
            expected: u16,
            found: u16,
        },
        InvalidStellarKeyEncodingVersion,
        AssetCodeTooLong,
        InvalidAssetCodeCharacter,
        InvalidBase32Character,
    }

    /// The ERC-20 result type.
    pub type Result<T> = core::result::Result<T, Error>;

    const MINIMUM_LIQUIDITY: u128 = 1;

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
        amount_to_send: Balance,
        amount_to_receive: Balance,
    }

    #[ink(event)]
    pub struct Sync {
        #[ink(topic)]
        reserve_0: Balance,
        #[ink(topic)]
        reserve_1: Balance,
    }

    #[ink(storage)]
    #[derive(SpreadAllocate)]
    pub struct Pair {
        reserve_0: Balance,
        reserve_1: Balance,

        asset_0: Asset,
        asset_1: Asset,
        total_supply: Balance,
        /// Mapping from owner to number of owned token.
        lp_balances: Mapping<AccountId, Balance>,
    }

    impl Pair {
        #[ink(constructor)]
        pub fn new(
            asset_code_0: String,
            issuer_0: String,
            asset_code_1: String,
            issuer_1: String,
        ) -> Self {
            let caller = Self::env().caller();

            let asset_code_0 =
                asset_from_string(asset_code_0).expect("Could not decode asset_code_0");
            let asset_code_1 =
                asset_from_string(asset_code_1).expect("Could not decode asset_code_1");

            let issuer_0 = decode_stellar_key::<String, ED25519_PUBLIC_KEY_BYTE_LENGTH>(
                issuer_0,
                ED25519_PUBLIC_KEY_VERSION_BYTE,
            )
            .expect("Could not decode issuer_0");
            let issuer_1 = decode_stellar_key::<String, ED25519_PUBLIC_KEY_BYTE_LENGTH>(
                issuer_1,
                ED25519_PUBLIC_KEY_VERSION_BYTE,
            )
            .expect("Could not decode issuer_1");

            // This call is required in order to correctly initialize the
            // `Mapping`s of our contract.
            let instance = ink_lang::utils::initialize_contract(|contract: &mut Self| {
                contract.asset_0 = (issuer_0, asset_code_0);
                contract.asset_1 = (issuer_1, asset_code_1);
                contract.reserve_0 = 0;
                contract.reserve_1 = 0;
                contract.total_supply = 0;
            });

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
            self.lp_balances.get(&owner).unwrap_or(0)
        }

        #[ink(message)]
        pub fn asset_1(&self) -> String {
            String::from_utf8(trim_zeros(&self.asset_0.1).to_vec()).unwrap()
        }

        #[ink(message)]
        pub fn issuer_1(&self) -> String {
            let issuer_0_encoded =
                encode_stellar_key(&self.asset_0.0, ED25519_PUBLIC_KEY_VERSION_BYTE);

            String::from_utf8(issuer_0_encoded).unwrap()
        }

        #[ink(message)]
        pub fn asset_2(&self) -> String {
            String::from_utf8(trim_zeros(&self.asset_1.1).to_vec()).unwrap()
        }

        #[ink(message)]
        pub fn issuer_2(&self) -> String {
            let issuer_1_encoded =
                encode_stellar_key(&self.asset_1.0, ED25519_PUBLIC_KEY_VERSION_BYTE);

            String::from_utf8(issuer_1_encoded).unwrap()
        }

        #[ink(message)]
        pub fn minimum_liquidity(&self) -> u128 {
            return MINIMUM_LIQUIDITY;
        }

        #[ink(message)]
        pub fn get_reserves(&self) -> (Balance, Balance) {
            return (self.reserve_0, self.reserve_1);
        }

        #[ink(message)]
        pub fn deposit_asset_1(&mut self, amount: Balance) -> Result<Balance> {
            let caller = self.env().caller();
            self.deposit(amount, self.asset_0, caller)
        }

        #[ink(message)]
        pub fn deposit_asset_2(&mut self, amount: Balance) -> Result<Balance> {
            let caller = self.env().caller();
            self.deposit(amount, self.asset_1, caller)
        }

        fn deposit(&mut self, amount: Balance, asset: Asset, to: AccountId) -> Result<Balance> {
            let contract = self.env().account_id();
            let from = to;

            let asset_0 = self.asset_0;
            let asset_1 = self.asset_1;

            let (reserve_0, reserve_1) = self.get_reserves();

            let balance_0 = self.balance_of(contract, asset_0);
            let balance_1 = self.balance_of(contract, asset_1);

            let (amount_0, amount_1) = if asset == asset_0 {
                (
                    amount,
                    if balance_0 > 0 {
                        amount * balance_1 / balance_0
                    } else {
                        amount
                    },
                )
            } else {
                (
                    if balance_1 > 0 {
                        amount * balance_0 / balance_1
                    } else {
                        amount
                    },
                    amount,
                )
            };

            let user_balance_0 = self.balance_of(from, asset_0);
            let user_balance_1 = self.balance_of(from, asset_1);
            if amount_0 > user_balance_0 {
                return Err(Error::InsufficientBalance0);
            }
            if amount_1 > user_balance_1 {
                return Err(Error::InsufficientBalance1);
            }

            let total_supply = self.total_supply;
            let liquidity: Balance;
            if total_supply == 0 {
                liquidity = sqrt(amount_0 * amount_1) - MINIMUM_LIQUIDITY;
                let address_zero = AccountId::from([0x01; 32]);
                self._mint(address_zero, MINIMUM_LIQUIDITY)?; // permanently lock first liquidity tokens
            } else {
                liquidity = core::cmp::min(
                    amount_0 * total_supply / reserve_0,
                    amount_1 * total_supply / reserve_1,
                );
            }

            if liquidity <= 0 {
                return Err(Error::InsufficientLiquidityMinted);
            }

            self.transfer_tokens(from, contract, asset_0, amount_0)?;
            self.transfer_tokens(from, contract, asset_1, amount_1)?;
            self._mint(from, liquidity)?;

            let balance_0 = self.balance_of(contract, asset_0);
            let balance_1 = self.balance_of(contract, asset_1);
            self._update(balance_0, balance_1, reserve_0, reserve_1)?;

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

            let user_lp_balance = self.lp_balance_of(to);
            if user_lp_balance < amount {
                return Err(Error::InsufficientLiquidityBalance);
            }

            let contract = self.env().account_id();
            let (reserve_0, reserve_1) = self.get_reserves();
            let asset_0 = self.asset_0;
            let asset_1 = self.asset_1;
            let balance_0 = self.balance_of(contract, asset_0);
            let balance_1 = self.balance_of(contract, asset_1);

            let amount_0 = amount * balance_0 / ((total_supply - amount) + amount);
            let amount_1 = amount * balance_1 / ((total_supply - amount) + amount);

            if !(amount_0 > 0 || amount_1 > 0) {
                return Err(Error::InsufficientLiquidityBurned);
            }

            self.transfer_tokens(contract, to, asset_0, amount_0)?;
            self.transfer_tokens(contract, to, asset_1, amount_1)?;
            self._burn(to, amount)?;

            let balance_0 = self.balance_of(contract, asset_0);
            let balance_1 = self.balance_of(contract, asset_1);
            self._update(balance_0, balance_1, reserve_0, reserve_1)?;

            self.env().emit_event(Burn {
                sender: self.env().caller(),
                amount_0,
                amount_1,
                to,
            });
            Ok((amount_0, amount_1))
        }

        #[ink(message)]
        pub fn swap_asset_1_for_asset_2(&mut self, amount_to_receive: Balance) -> Result<()> {
            let caller = self.env().caller();
            self._swap(caller, amount_to_receive, self.asset_1)
        }

        #[ink(message)]
        pub fn swap_asset_2_for_asset_1(&mut self, amount_to_receive: Balance) -> Result<()> {
            let caller = self.env().caller();
            self._swap(caller, amount_to_receive, self.asset_0)
        }

        fn _swap(
            &mut self,
            from: AccountId,
            amount_to_receive: Balance,
            asset_to_receive: Asset,
        ) -> Result<()> {
            if amount_to_receive <= 0 {
                return Err(Error::InsufficientOutputAmount);
            }

            let asset_0 = self.asset_0;
            let asset_1 = self.asset_1;

            let (reserve_0, reserve_1) = self.get_reserves();
            if asset_to_receive == asset_0 && amount_to_receive > reserve_0 {
                return Err(Error::InsufficientLiquidity);
            } else if asset_to_receive == asset_1 && amount_to_receive > reserve_1 {
                return Err(Error::InsufficientLiquidity);
            }

            let contract = self.env().account_id();
            let balance_0 = self.balance_of(contract, asset_0);
            let balance_1 = self.balance_of(contract, asset_1);

            let (amount_to_send, asset_to_send) = if asset_to_receive == asset_0 {
                (
                    amount_to_receive * balance_1 / (balance_0 - amount_to_receive),
                    asset_1,
                )
            } else {
                (
                    amount_to_receive * balance_0 / (balance_1 - amount_to_receive),
                    asset_0,
                )
            };

            self.transfer_tokens(from, contract, asset_to_send, amount_to_send)?;
            self.transfer_tokens(contract, from, asset_to_receive, amount_to_receive)?;

            let balance_0 = self.balance_of(contract, asset_0);
            let balance_1 = self.balance_of(contract, asset_1);

            self._update(balance_0, balance_1, reserve_0, reserve_1)?;
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
            asset: Asset,
            amount: Balance,
        ) -> Result<()> {
            let from_balance = self.balance_of(from, asset);
            if from_balance < amount {
                return Err(Error::InsufficientBalance);
            }

            self.env()
                .extension()
                .transfer_balance(from, to, asset, amount);
            Ok(())
        }

        pub fn balance_of(&self, owner: AccountId, asset: Asset) -> Balance {
            let balance = match self.env().extension().fetch_balance(owner, asset) {
                Ok(balance) => balance,
                // Err(err) => Err(BalanceReadErr::FailGetBalance),
                Err(_) => 0,
            };
            return balance;
        }

        fn _update(
            &mut self,
            balance_0: Balance,
            balance_1: Balance,
            reserve_0: Balance,
            reserve_1: Balance,
        ) -> Result<()> {
            self.reserve_0 = balance_0;
            self.reserve_1 = balance_1;
            self.env().emit_event(Sync {
                reserve_0,
                reserve_1,
            });
            Ok(())
        }

        fn _mint(&mut self, to: AccountId, value: Balance) -> Result<()> {
            self.total_supply += value;
            let balance = self.lp_balance_of(to);
            self.lp_balances.insert(to, &(balance + value));
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
            self.lp_balances.insert(from, &(balance - value));
            self.env().emit_event(Transfer {
                from: Some(from),
                to: None,
                value,
            });
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;
        // use ink_env::debug_println;
        use ink_lang as ink;
        use lazy_static::lazy_static;
        // use scale::{Decode, Encode, Input};
        // use std::sync::PoisonError;

        const TOKEN_0: Asset = ([0; 32], [0; 12]);
        const TOKEN_1: Asset = ([1; 32], [1; 12]);

        const TOKEN_0_STRING: &str = "EUR";
        const ISSUER_0_STRING: &str = "GAP4SFKVFVKENJ7B7VORAYKPB3CJIAJ2LMKDJ22ZFHIAIVYQOR6W3CXF";
        const TOKEN_1_STRING: &str = "USDC";
        const ISSUER_1_STRING: &str = "GAP4SFKVFVKENJ7B7VORAYKPB3CJIAJ2LMKDJ22ZFHIAIVYQOR6W3CXF";

        // TODO build hashmap for currency and
        type Currency = [u8; 32];
        type FetchBalanceInput = [u8; 32 + 32 + 12]; // 1-> owner:AccountId, 2-> asset_issuer: [u8;32], 3-> asset_code: [u8;12]
        type TransferBalanceInput = [u8; 32 + 32 + 32 + 12 + 16]; // 1-> from: AccoundId, 2-> to: AccountId, 3-> asset_issuer: [u8;32], 4-> asset_code: [u8;12], 5-> balance: u128
        type BalanceMapping = Mapping<(AccountId, Currency), Balance>;

        // type TestResult<T> = std::result::Result<T, TestError>;

        // #[derive(Debug, thiserror::Error)]
        // enum TestError {
        //     #[error("{0:?}")]
        //     AmmError(Error),
        //     #[error("Lock poisoned: {0}")]
        //     LockPoisoned(String),
        // }

        // impl<T> From<PoisonError<T>> for TestError {
        //     fn from(err: PoisonError<T>) -> Self {
        //         Self::LockPoisoned(err.to_string())
        //     }
        // }

        // lazy_static! {
        //     static ref BALANCES: Arc<RwLock<BalanceMapping>> =
        //         Arc::new(RwLock::new(Mapping::default()));
        // }

        lazy_static! {
            static ref BALANCES: BalanceMapping = Mapping::default();
        }

        // fn set_balances(balances: BalanceMapping) -> TestResult<()> {
        //     *Arc::clone(&BALANCES).write()? = balances;
        //     Ok(())
        // }

        struct MockedBalanceExtension {
            balances: &'static BalanceMapping,
        }

        impl ink_env::test::ChainExtension for MockedBalanceExtension {
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
            fn call(&mut self, mut _input: &[u8], output: &mut Vec<u8>) -> u32 {
                let input: FetchBalanceInput =
                    <FetchBalanceInput as scale::Decode>::decode(&mut _input).unwrap();

                let mut account_array: [u8; 32] = Default::default();
                account_array.copy_from_slice(&input[0..32]);
                let account_id = AccountId::from(account_array);

                let mut issuer_array: [u8; 32] = Default::default();
                issuer_array.copy_from_slice(&input[32..64]);

                let mut asset_code_array: [u8; 12] = Default::default();
                asset_code_array.copy_from_slice(&input[96..108]);

                let balance = &BALANCES.get(&(account_id, issuer_array)).unwrap_or(0);
                // let balance = Arc::clone(&BALANCES)
                //     .read()
                //     .unwrap()
                //     .get(&(account_id, issuer_array))
                //     .unwrap_or(0);

                scale::Encode::encode_to(&balance, output);

                // let balance: Balance = 1000000;
                // let ret = balance.encode();
                // debug_println!("MockedBalanceExtension::call 1101");

                0 // 0 is error code
            }
        }

        struct MockedTransferExtension {
            balances: &'static BalanceMapping,
        }

        impl ink_env::test::ChainExtension for MockedTransferExtension {
            /// The static function id of the chain extension.
            fn func_id(&self) -> u32 {
                1102
            }

            /// The chain extension is called with the given input.
            ///
            /// Returns an error code and may fill the `output` buffer with a
            /// SCALE encoded result. The error code is taken from the
            /// `ink_env::chain_extension::FromStatusCode` implementation for
            /// `RandomReadErr`.
            fn call(&mut self, _input: &[u8], output: &mut Vec<u8>) -> u32 {
                // let mut from_account_array: [u8; 32] = Default::default();
                // from_account_array.copy_from_slice(&input[0..32]);
                // let from_account_id = AccountId::from(from_account_array);

                // let mut to_account_array: [u8; 32] = Default::default();
                // to_account_array.copy_from_slice(&input[32..64]);
                // let to_account_id = AccountId::from(to_account_array);

                // let mut issuer_array: [u8; 32] = Default::default();
                // issuer_array.copy_from_slice(&input[64..96]);

                // let mut asset_code_array: [u8; 12] = Default::default();
                // asset_code_array.copy_from_slice(&input[96..108]);

                // let mut amount_array: [u8; 16] = Default::default();
                // amount_array.copy_from_slice(&input[108..]);
                // let amount: u128 = u128::from_le_bytes(amount_array);

                let dispatch_result: Result<()> = Ok(());
                // let ret = dispatch_result.encode();
                scale::Encode::encode_to(&dispatch_result, output);
                // debug_println!("MockedTransferExtension::call 1102");

                0 // 0 is error code
            }
        }

        /// The default constructor does its job.
        #[ink::test]
        fn new_works() {
            // Constructor works.
            let pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

            let contract_balance_0 = pair.reserve_0;
            let contract_balance_1 = pair.reserve_1;
            assert_eq!(contract_balance_0, contract_balance_1);
        }

        #[ink::test]
        fn asset_1_works() {
            // Constructor works.
            let pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

            assert_eq!(pair.asset_1(), "EUR");
        }

        #[ink::test]
        fn issuer_1_works() {
            // Constructor works.
            let pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

            assert_eq!(
                pair.issuer_1(),
                "GAP4SFKVFVKENJ7B7VORAYKPB3CJIAJ2LMKDJ22ZFHIAIVYQOR6W3CXF"
            );
        }

        #[ink::test]
        fn balance_of_works() {
            // let balances = &Arc::clone(&BALANCES).read().unwrap();
            let balances = &BALANCES;
            ink_env::test::register_chain_extension(MockedBalanceExtension { balances });
            ink_env::test::register_chain_extension(MockedTransferExtension { balances });

            let to = AccountId::from([0x01; 32]);
            let pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );
            println!("balance of: balance: {}", pair.balance_of(to, TOKEN_0));
            assert_eq!(pair.balance_of(to, TOKEN_0), 0);
        }

        #[ink::test]
        fn deposit_works_for_balanced_pair() {
            let balances = &BALANCES;
            ink_env::test::register_chain_extension(MockedBalanceExtension { balances });
            ink_env::test::register_chain_extension(MockedTransferExtension { balances });

            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000;
            let mut pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

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
            let balances = &BALANCES;
            ink_env::test::register_chain_extension(MockedBalanceExtension { balances });
            ink_env::test::register_chain_extension(MockedTransferExtension { balances });
            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000;
            let mut pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

            pair.swap_asset_2_for_asset_1(100)
                .expect("Swap did not work");

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
            let balances = &BALANCES;
            ink_env::test::register_chain_extension(MockedBalanceExtension { balances });
            ink_env::test::register_chain_extension(MockedTransferExtension { balances });

            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

            let result = pair.withdraw(1, to);
            assert_eq!(Err(Error::WithdrawWithoutSupply), result);

            let gained_lp = pair.deposit(5_000, TOKEN_0, to).expect("Could not deposit");
            // try withdrawing more LP than account has
            let result = pair.withdraw(gained_lp + 2, to);
            assert_eq!(Err(Error::InsufficientLiquidityBalance), result);
        }

        #[ink::test]
        fn withdraw_works() {
            let balances = &BALANCES;
            ink_env::test::register_chain_extension(MockedBalanceExtension { balances });
            ink_env::test::register_chain_extension(MockedTransferExtension { balances });

            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

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
            let balances = &BALANCES;
            let mocked_balance_extension = MockedBalanceExtension { balances };
            let mocked_transfer_extension = MockedTransferExtension { balances };

            ink_env::test::register_chain_extension(mocked_balance_extension);
            ink_env::test::register_chain_extension(mocked_transfer_extension);

            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

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
            let balances = &BALANCES;
            ink_env::test::register_chain_extension(MockedBalanceExtension { balances });
            ink_env::test::register_chain_extension(MockedTransferExtension { balances });

            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

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

            let result = pair.swap_asset_2_for_asset_1(swap_amount);
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

            let result = pair.swap_asset_1_for_asset_2(swap_amount);
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
            let balances = &BALANCES;
            ink_env::test::register_chain_extension(MockedBalanceExtension { balances });
            ink_env::test::register_chain_extension(MockedTransferExtension { balances });

            let to = AccountId::from([0x01; 32]);

            let initial_supply = 1_000_000;
            let mut pair = Pair::new(
                TOKEN_0_STRING.to_string(),
                ISSUER_0_STRING.to_string(),
                TOKEN_1_STRING.to_string(),
                ISSUER_1_STRING.to_string(),
            );

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

            let result = pair.swap_asset_2_for_asset_1(swap_amount);
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
            let result = pair.swap_asset_2_for_asset_1(swap_amount);
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
