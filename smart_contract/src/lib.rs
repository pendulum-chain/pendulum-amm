#![cfg_attr(not(feature = "std"), no_std)]

use ink_env::Environment;
use ink_lang as ink;

extern crate alloc;

pub type AssetCode = [u8; 12];
pub type IssuerId = [u8; 32]; // encoded 32-bit array of 56 character stellar issuer (public key)
pub type Asset = (IssuerId, AssetCode);

#[ink::chain_extension]
pub trait BalanceExtension {
	type ErrorCode = BalanceReadErr;

	#[ink(extension = 1101, returns_result = false)]
	fn fetch_balance(of: ink_env::AccountId, asset: Asset) -> u128;

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
			2 => Err(Self::FailTransferBalance),
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
	use crate::{amm::Error, AssetCode};

	pub fn asset_from_string(str: ink_prelude::string::String) -> Result<AssetCode, Error> {
		let str: &[u8] = str.as_ref();
		if str.len() > 12 {
			return Err(Error::AssetCodeTooLong)
		}

		if !str.iter().all(|char| {
			let char = char::from(*char);
			char.is_ascii_alphanumeric()
		}) {
			return Err(Error::InvalidAssetCodeCharacter)
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
			let value_5bit = if shift == 8 { carry } else { carry | ((*byte) >> shift) };
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
				break
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
				return Err(Error::InvalidBase32Character)
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
			return Err(Error::InvalidStellarKeyEncoding)
		}

		let array_length = decoded_array.len();
		if array_length != 3 + BYTE_LENGTH {
			return Err(Error::InvalidStellarKeyEncodingLength)
		}

		let crc_value = ((decoded_array[array_length - 1] as u16) << 8) |
			decoded_array[array_length - 2] as u16;
		let expected_crc_value = crc(&decoded_array[..array_length - 2]);
		if crc_value != expected_crc_value {
			return Err(Error::InvalidStellarKeyChecksum {
				expected: expected_crc_value,
				found: crc_value,
			})
		}

		let expected_version = version_byte;
		if decoded_array[0] != expected_version {
			return Err(Error::InvalidStellarKeyEncodingVersion)
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

	use crate::{
		key_encoding::{
			decode_stellar_key, encode_stellar_key, ED25519_PUBLIC_KEY_BYTE_LENGTH,
			ED25519_PUBLIC_KEY_VERSION_BYTE,
		},
		util::{asset_from_string, trim_zeros},
		Asset,
	};
	use ink_prelude::string::String;
	use ink_storage::{traits::SpreadAllocate, Mapping};
	use num_integer::sqrt;

	/// The ERC-20 error types.
	#[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
	#[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
	pub enum Error {
		Forbidden,
		/// Returned if not enough balance to fulfill a request is available.
		InsufficientBalance,
		/// Returned if not enough allowance to fulfill a request is available.
		InsufficientAllowance,
		InsufficientLiquidity,
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
	#[derive(SpreadAllocate)]
	pub struct Pair {
		asset_0: Asset,
		asset_1: Asset,

		reserve_0: Balance,
		reserve_1: Balance,
		block_timestamp_last: u64,

		price_0_cumulative_last: Balance,
		price_1_cumulative_last: Balance,
		k_last: Balance,

		fee_to: Option<AccountId>,
		fee_to_setter: AccountId,

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
			// TODO maybe change fee_to_setter to other address
			let fee_to_setter = caller;

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
				contract.block_timestamp_last = 0;
				contract.price_0_cumulative_last = 0;
				contract.price_1_cumulative_last = 0;
				contract.k_last = 0;
				contract.fee_to = None;
				contract.fee_to_setter = fee_to_setter;
				contract.total_supply = 0;
			});

			Self::env().emit_event(Transfer { from: None, to: Some(caller), value: 0 });

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
			return MINIMUM_LIQUIDITY
		}

		#[ink(message)]
		pub fn get_reserves(&self) -> (Balance, Balance, u64) {
			return (self.reserve_0, self.reserve_1, self.block_timestamp_last)
		}

		#[ink(message)]
		pub fn price_0_cumulative_last(&self) -> u128 {
			return self.price_0_cumulative_last
		}

		#[ink(message)]
		pub fn price_1_cumulative_last(&self) -> u128 {
			return self.price_1_cumulative_last
		}

		#[ink(message)]
		pub fn k_last(&self) -> u128 {
			return self.k_last
		}

		#[ink(message)]
		pub fn set_fee_to(&mut self, fee_to: AccountId) -> Result<()> {
			let caller = self.env().caller();
			if !(caller == self.fee_to_setter) {
				return Err(Error::Forbidden)
			}
			self.fee_to = Some(fee_to);
			Ok(())
		}

		#[ink(message)]
		/// Force balances to match reserves
		pub fn skim(&mut self, to: AccountId) -> Result<()> {
			let contract = self.env().account_id();
			let amount_0_calc = self.balance_of(contract, self.asset_0).checked_sub(self.reserve_0);
			if let Some(amount_0) = amount_0_calc {
				self.transfer_tokens(contract, to, self.asset_0, amount_0)?;
			}

			let amount_1_calc = self.balance_of(contract, self.asset_1).checked_sub(self.reserve_1);
			if let Some(amount_1) = amount_1_calc {
				self.transfer_tokens(contract, to, self.asset_1, amount_1)?;
			}
			Ok(())
		}

		#[ink(message)]
		pub fn sync(&mut self) -> Result<()> {
			let contract = self.env().account_id();
			let balance_0 = self.balance_of(contract, self.asset_0);
			let balance_1 = self.balance_of(contract, self.asset_1);
			self._update(balance_0, balance_1, self.reserve_0, self.reserve_1)?;
			Ok(())
		}

		/// Add liquidity
		#[ink(message)]
		pub fn deposit_asset_1(&mut self, amount: Balance) -> Result<Balance> {
			let caller = self.env().caller();
			let contract = self.env().account_id();

			let (reserve_0, reserve_1, _) = self.get_reserves();
			let (amount_0, amount_1) = if reserve_0 == 0 && reserve_1 == 0 {
				(amount, amount)
			} else {
				let amount_0_desired = amount;
				let amount_1_optimal =
					self.quote(amount_0_desired, self.reserve_0, self.reserve_1)?;

				(amount_0_desired, amount_1_optimal)
			};

			self.transfer_tokens(caller, contract, self.asset_0, amount_0)?;
			self.transfer_tokens(caller, contract, self.asset_1, amount_1)?;

			self.mint(caller)
		}

		/// Add liquidity
		#[ink(message)]
		pub fn deposit_asset_2(&mut self, amount: Balance) -> Result<Balance> {
			let caller = self.env().caller();
			let contract = self.env().account_id();

			let (reserve_0, reserve_1, _) = self.get_reserves();
			let (amount_0, amount_1) = if reserve_0 == 0 && reserve_1 == 0 {
				(amount, amount)
			} else {
				let amount_1_desired = amount;
				let amount_0_optimal =
					self.quote(amount_1_desired, self.reserve_1, self.reserve_0)?;

				(amount_0_optimal, amount_1_desired)
			};

			self.transfer_tokens(caller, contract, self.asset_0, amount_0)?;
			self.transfer_tokens(caller, contract, self.asset_1, amount_1)?;

			self.mint(caller)
		}

		fn mint(&mut self, to: AccountId) -> Result<Balance> {
			let contract = self.env().account_id();
			let (reserve_0, reserve_1, _) = self.get_reserves();

			let balance_0 = self.balance_of(contract, self.asset_0);
			let balance_1 = self.balance_of(contract, self.asset_1);
			let amount_0 = balance_0.checked_sub(reserve_0).unwrap_or(0);
			let amount_1 = balance_1.checked_sub(reserve_1).unwrap_or(0);

			let fee_on = self._mint_fee(reserve_0, reserve_1)?;
			let total_supply = self.total_supply;
			let liquidity: Balance;
			if total_supply == 0 {
				liquidity =
					sqrt(amount_0.saturating_mul(amount_1)).saturating_sub(MINIMUM_LIQUIDITY);
				let address_zero = AccountId::from([0x0; 32]);
				self._mint(address_zero, MINIMUM_LIQUIDITY)?; // permanently lock first liquidity tokens
			} else {
				liquidity = core::cmp::min(
					amount_0.saturating_mul(total_supply).saturating_div(reserve_0),
					amount_1.saturating_mul(total_supply).saturating_div(reserve_1),
				);
			}

			if !(liquidity > 0) {
				return Err(Error::InsufficientLiquidityMinted)
			}

			self._mint(to, liquidity)?;

			self._update(balance_0, balance_1, reserve_0, reserve_1)?;
			if fee_on {
				self.k_last = reserve_0.saturating_mul(reserve_1);
			}

			self.env().emit_event(Mint { sender: self.env().caller(), amount_0, amount_1 });

			Ok(liquidity)
		}

		/// Remove Liquidity
		#[ink(message)]
		pub fn withdraw(&mut self, amount: Balance) -> Result<(Balance, Balance)> {
			let caller = self.env().caller();
			let contract = self.env().account_id();

			let total_supply = self.total_supply;
			if total_supply == 0 {
				return Err(Error::WithdrawWithoutSupply)
			}

			self._transfer_liquidity(caller, contract, amount)?;

			self.burn(caller)
		}

		fn burn(&mut self, to: AccountId) -> Result<(Balance, Balance)> {
			let contract = self.env().account_id();
			let (reserve_0, reserve_1, _) = self.get_reserves();
			let asset_0 = self.asset_0;
			let asset_1 = self.asset_1;
			let balance_0 = self.balance_of(contract, asset_0);
			let balance_1 = self.balance_of(contract, asset_1);
			let liquidity = self.lp_balance_of(contract);

			let fee_on = self._mint_fee(reserve_0, reserve_1)?;
			let total_supply = self.total_supply;
			let amount_0 = liquidity.saturating_mul(balance_0).saturating_div(total_supply);
			let amount_1 = liquidity.saturating_mul(balance_1).saturating_div(total_supply);

			if !(amount_0 > 0 && amount_1 > 0) {
				return Err(Error::InsufficientLiquidityBurned)
			}

			self._burn(contract, liquidity)?;
			self.transfer_tokens(contract, to, asset_0, amount_0)?;
			self.transfer_tokens(contract, to, asset_1, amount_1)?;

			let balance_0 = self.balance_of(contract, asset_0);
			let balance_1 = self.balance_of(contract, asset_1);
			self._update(balance_0, balance_1, reserve_0, reserve_1)?;

			if fee_on {
				self.k_last = reserve_0.saturating_mul(reserve_1);
			}

			self.env()
				.emit_event(Burn { sender: self.env().caller(), amount_0, amount_1, to });
			Ok((amount_0, amount_1))
		}

		/// Swap
		#[ink(message)]
		pub fn swap_asset_1_for_asset_2(&mut self, amount_to_receive: Balance) -> Result<()> {
			let caller = self.env().caller();
			let contract = self.env().account_id();

			let amount_0_in =
				self.get_amount_in(amount_to_receive, self.reserve_0, self.reserve_1)?; // TODO check if the reserves are in correct order
			self.transfer_tokens(caller, contract, self.asset_0, amount_0_in)?;

			self._swap(0, amount_to_receive, caller)
		}

		/// Swap
		#[ink(message)]
		pub fn swap_asset_2_for_asset_1(&mut self, amount_to_receive: Balance) -> Result<()> {
			let caller = self.env().caller();
			let contract = self.env().account_id();

			let amount_1_in =
				self.get_amount_in(amount_to_receive, self.reserve_1, self.reserve_0)?;
			self.transfer_tokens(caller, contract, self.asset_1, amount_1_in)?;

			self._swap(amount_to_receive, 0, caller)
		}

		fn _swap(
			&mut self,
			amount_0_out: Balance,
			amount_1_out: Balance,
			to: AccountId,
		) -> Result<()> {
			if !(amount_0_out > 0 || amount_1_out > 0) {
				return Err(Error::InsufficientOutputAmount)
			}

			let asset_0 = self.asset_0;
			let asset_1 = self.asset_1;

			let (reserve_0, reserve_1, _) = self.get_reserves();
			if !(amount_0_out < reserve_0 && amount_1_out < reserve_1) {
				return Err(Error::InsufficientLiquidity)
			}

			// optimistically transfer tokens
			let contract = self.env().account_id();
			if amount_0_out > 0 {
				self.transfer_tokens(contract, to, asset_0, amount_0_out)?;
			}
			if amount_1_out > 0 {
				self.transfer_tokens(contract, to, asset_1, amount_1_out)?;
			}

			let balance_0 = self.balance_of(contract, asset_0);
			let balance_1 = self.balance_of(contract, asset_1);

			let amount_0_in = if balance_0 > reserve_0.saturating_sub(amount_0_out) {
				balance_0.saturating_sub(reserve_0.saturating_sub(amount_0_out))
			} else {
				0
			};
			let amount_1_in = if balance_1 > reserve_1.saturating_sub(amount_1_out) {
				balance_1.saturating_sub(reserve_1.saturating_sub(amount_1_out))
			} else {
				0
			};

			if !(amount_0_in > 0 || amount_1_in > 0) {
				return Err(Error::InsufficientInputAmount)
			}

			let balance_0_adjusted =
				balance_0.saturating_mul(1000).saturating_sub(amount_0_in.saturating_mul(3));
			let balance_1_adjusted =
				balance_1.saturating_mul(1000).saturating_sub(amount_1_in.saturating_mul(3));

			if !(balance_0_adjusted.saturating_mul(balance_1_adjusted) >=
				reserve_0.saturating_mul(reserve_1).saturating_mul(1000 * 1000))
			{
				return Err(Error::InvalidK)
			}

			let balance_0 = self.balance_of(contract, asset_0);
			let balance_1 = self.balance_of(contract, asset_1);

			self._update(balance_0, balance_1, reserve_0, reserve_1)?;
			self.env().emit_event(Swap {
				sender: self.env().caller(),
				to,
				amount_0_in,
				amount_1_in,
				amount_0_out,
				amount_1_out,
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
				return Err(Error::InsufficientBalance)
			}

			self.env().extension().transfer_balance(from, to, asset, amount);
			Ok(())
		}

		pub fn balance_of(&self, owner: AccountId, asset: Asset) -> Balance {
			let balance = match self.env().extension().fetch_balance(owner, asset) {
				Ok(balance) => balance,
				// Err(err) => Err(BalanceReadErr::FailGetBalance),
				Err(_) => 0,
			};
			return balance
		}

		fn _update(
			&mut self,
			balance_0: Balance,
			balance_1: Balance,
			reserve_0: Balance,
			reserve_1: Balance,
		) -> Result<()> {
			let block_timestamp = self.env().block_timestamp();
			let time_elapsed = block_timestamp.overflowing_sub(self.block_timestamp_last).0; // overflow is desired

			if time_elapsed > 0 && reserve_0 != 0 && reserve_1 != 0 {
				// * never overflows, and + overflow is desired
				self.price_0_cumulative_last = self
					.price_0_cumulative_last
					.overflowing_add(
						reserve_1
							.checked_div(reserve_0)
							.unwrap_or(0u128)
							.saturating_mul(time_elapsed.into()),
					)
					.0;
				self.price_1_cumulative_last = self
					.price_1_cumulative_last
					.overflowing_add(
						reserve_0
							.checked_div(reserve_1)
							.unwrap_or(0u128)
							.saturating_mul(time_elapsed.into()),
					)
					.0;
			}

			self.reserve_0 = balance_0;
			self.reserve_1 = balance_1;
			self.block_timestamp_last = block_timestamp;
			self.env().emit_event(Sync { reserve_0, reserve_1 });
			Ok(())
		}

		fn _mint_fee(&mut self, reserve_0: Balance, reserve_1: Balance) -> Result<bool> {
			let fee_on = self.fee_to.is_some();
			if let Some(fee_to) = self.fee_to {
				if self.k_last != 0 {
					let root_k = sqrt(reserve_0.saturating_mul(reserve_1));
					let root_k_last = sqrt(self.k_last);
					if root_k > root_k_last {
						let numerator =
							self.total_supply.saturating_mul(root_k.saturating_sub(root_k_last));
						let denominator = root_k.saturating_mul(5).saturating_add(root_k_last);
						let liquidity = numerator.saturating_div(denominator);
						if liquidity > 0 {
							self._mint(fee_to, liquidity)?;
						}
					}
				}
			} else if self.k_last != 0 {
				self.k_last = 0;
			}
			Ok(fee_on)
		}

		fn _mint(&mut self, to: AccountId, value: Balance) -> Result<()> {
			self.total_supply += value;
			let balance = self.lp_balance_of(to);
			self.lp_balances.insert(to, &(balance.saturating_add(value)));
			self.env().emit_event(Transfer { from: None, to: Some(to), value });
			Ok(())
		}

		fn _burn(&mut self, from: AccountId, value: Balance) -> Result<()> {
			self.total_supply -= value;
			let balance = self.lp_balance_of(from);
			self.lp_balances.insert(from, &(balance.saturating_sub(value)));
			self.env().emit_event(Transfer { from: Some(from), to: None, value });
			Ok(())
		}

		fn _transfer_liquidity(
			&mut self,
			from: AccountId,
			to: AccountId,
			amount: Balance,
		) -> Result<()> {
			let balance = self.lp_balance_of(from);
			if balance < amount {
				return Err(Error::InsufficientBalance)
			}
			self.lp_balances.insert(from, &(balance.saturating_sub(amount)));
			self.lp_balances.insert(to, &(self.lp_balance_of(to).saturating_add(amount)));
			self.env()
				.emit_event(Transfer { from: Some(from), to: Some(to), value: amount });
			Ok(())
		}

		fn get_amount_out(
			&self,
			amount_in: Balance,
			reserve_in: Balance,
			reserve_out: Balance,
		) -> Result<Balance> {
			if !(amount_in > 0) {
				return Err(Error::InsufficientInputAmount)
			}
			if !(reserve_in > 0 && reserve_out > 0) {
				return Err(Error::InsufficientLiquidity)
			}
			let amount_in_with_fee = amount_in.saturating_mul(997);
			let numerator = amount_in_with_fee.saturating_mul(reserve_out);
			let denominator = reserve_in.saturating_mul(1000).saturating_add(amount_in_with_fee);
			Ok(numerator.saturating_div(denominator))
		}

		fn get_amount_in(
			&self,
			amount_out: Balance,
			reserve_in: Balance,
			reserve_out: Balance,
		) -> Result<Balance> {
			if !(amount_out > 0) {
				return Err(Error::InsufficientOutputAmount)
			}
			if !(reserve_in > 0 && reserve_out > 0) {
				return Err(Error::InsufficientLiquidity)
			}
			let numerator = reserve_in.saturating_mul(reserve_out).saturating_mul(1000);
			let denominator = reserve_out.saturating_sub(amount_out).saturating_mul(997);
			Ok(numerator.saturating_div(denominator).saturating_add(1))
		}

		fn quote(
			&self,
			amount_a: Balance,
			reserve_a: Balance,
			reserve_b: Balance,
		) -> Result<Balance> {
			if !(amount_a > 0) {
				return Err(Error::InsufficientInputAmount)
			}
			if !(reserve_a > 0 && reserve_b > 0) {
				return Err(Error::InsufficientLiquidity)
			}
			let amount_b = amount_a.saturating_mul(reserve_b).saturating_div(reserve_a);
			Ok(amount_b)
		}
	}

	#[cfg(test)]
	mod tests {
		/// Imports all the definitions from the outer scope so we can use them here.
		use super::*;
		use ink_env::debug_println;
		use ink_lang as ink;
		use ink_prelude::collections::HashMap;
		use lazy_static::lazy_static;
		use serial_test::serial;
		use std::sync::Mutex;

		const ASSET_CODE_0_STRING: &str = "EUR";
		const ISSUER_0_STRING: &str = "GAP4SFKVFVKENJ7B7VORAYKPB3CJIAJ2LMKDJ22ZFHIAIVYQOR6W3CXF";
		const ASSET_CODE_1_STRING: &str = "USDC";
		const ISSUER_1_STRING: &str = "GAP4SFKVFVKENJ7B7VORAYKPB3CJIAJ2LMKDJ22ZFHIAIVYQOR6W3CXF";

		// Used for initializing account id of test account
		// Should not be [0x01; 32] because that's the contract address
		const TO_BYTE_ARRAY: [u8; 32] = [0x05; 32];

		type BalanceMapping = HashMap<(AccountId, Asset), Balance>;

		lazy_static! {
			static ref BALANCES: Mutex<BalanceMapping> = Mutex::new(HashMap::default());
		}

		struct MockedBalanceExtension;
		impl ink_env::test::ChainExtension for MockedBalanceExtension {
			fn func_id(&self) -> u32 {
				1101
			}

			fn call(&mut self, mut _input: &[u8], output: &mut Vec<u8>) -> u32 {
				// skip first two bytes because they don't contain input data
				let input = &_input[2..];

				let mut account_array: [u8; 32] = Default::default();
				account_array.copy_from_slice(&input[0..32]);
				let account_id = AccountId::from(account_array);

				let mut issuer_array: [u8; 32] = Default::default();
				issuer_array.copy_from_slice(&input[32..64]);

				let mut asset_code_array: [u8; 12] = Default::default();
				asset_code_array.copy_from_slice(&input[64..]);

				let asset: Asset = (issuer_array, asset_code_array);

				let map = BALANCES.lock().unwrap();
				let balance = map.get(&(account_id, asset)).unwrap_or(&0);

				scale::Encode::encode_to(&balance, output);

				0 // 0 is error code
			}
		}

		struct MockedTransferExtension;
		impl ink_env::test::ChainExtension for MockedTransferExtension {
			fn func_id(&self) -> u32 {
				1102
			}

			fn call(&mut self, mut _input: &[u8], output: &mut Vec<u8>) -> u32 {
				// skip first two bytes because they don't contain input data
				let input = &_input[2..];

				let mut from_account_array: [u8; 32] = Default::default();
				from_account_array.copy_from_slice(&input[0..32]);
				let from_account_id = AccountId::from(from_account_array);

				let mut to_account_array: [u8; 32] = Default::default();
				to_account_array.copy_from_slice(&input[32..64]);
				let to_account_id = AccountId::from(to_account_array);

				let mut issuer_array: [u8; 32] = Default::default();
				issuer_array.copy_from_slice(&input[64..96]);

				let mut asset_code_array: [u8; 12] = Default::default();
				asset_code_array.copy_from_slice(&input[96..108]);

				let mut amount_array: [u8; 16] = Default::default();
				amount_array.copy_from_slice(&input[108..]);
				let amount: u128 = u128::from_le_bytes(amount_array);

				let asset: Asset = (issuer_array, asset_code_array);

				// emulate transfer
				let mut map = BALANCES.lock().unwrap();
				map.entry((from_account_id, asset))
					.and_modify(|e| *e = e.saturating_sub(amount))
					.or_insert(0);

				map.entry((to_account_id, asset))
					.and_modify(|e| *e = e.saturating_add(amount))
					.or_insert(amount);

				let dispatch_result: Result<()> = Ok(());
				scale::Encode::encode_to(&dispatch_result, output);

				0 // 0 is error code
			}
		}

		fn reset_map() {
			let mut map = BALANCES.lock().unwrap();
			map.clear();
		}

		fn get_default_pair() -> Pair {
			Pair::new(
				ASSET_CODE_0_STRING.to_string(),
				ISSUER_0_STRING.to_string(),
				ASSET_CODE_1_STRING.to_string(),
				ISSUER_1_STRING.to_string(),
			)
		}

		fn add_supply_for_account(account_id: AccountId, supply: Balance, pair: &Pair) {
			let mut map = BALANCES.lock().unwrap();
			map.insert((account_id, pair.asset_0), supply);
			map.insert((account_id, pair.asset_1), supply);
		}

		/// The default constructor does its job.
		#[ink::test]
		#[serial]
		fn new_works() {
			// Constructor works.
			let pair = get_default_pair();

			let contract_balance_0 = pair.reserve_0;
			let contract_balance_1 = pair.reserve_1;
			assert_eq!(contract_balance_0, contract_balance_1);
		}

		#[ink::test]
		#[serial]
		fn asset_1_works() {
			// Constructor works.
			let pair = get_default_pair();

			assert_eq!(pair.asset_1(), "EUR");
		}

		#[ink::test]
		#[serial]
		fn issuer_1_works() {
			// Constructor works.
			let pair = get_default_pair();

			assert_eq!(pair.issuer_1(), "GAP4SFKVFVKENJ7B7VORAYKPB3CJIAJ2LMKDJ22ZFHIAIVYQOR6W3CXF");
		}

		#[ink::test]
		#[serial]
		fn balance_of_works() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let pair = get_default_pair();
			let to = AccountId::from(TO_BYTE_ARRAY);

			println!("balance of: balance: {}", pair.balance_of(to, pair.asset_0));
			assert_eq!(pair.balance_of(to, pair.asset_0), 0);
		}

		#[ink::test]
		#[serial]
		fn deposit_works_for_balanced_pair() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let to = AccountId::from(TO_BYTE_ARRAY);
			ink_env::test::set_caller::<ink_env::DefaultEnvironment>(to);

			let mut pair = get_default_pair();
			let initial_supply = 1_000;
			add_supply_for_account(to, initial_supply, &pair);

			let deposit_amount = 100;

			let user_balance_0_pre_deposit = pair.balance_of(to, pair.asset_0);
			let user_balance_1_pre_deposit = pair.balance_of(to, pair.asset_1);

			let result = pair.deposit_asset_1(deposit_amount);
			let gained_lp = result.expect("Could not unwrap gained lp");
			assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

			let user_balance_0_post_deposit = pair.balance_of(to, pair.asset_0);
			let user_balance_1_post_deposit = pair.balance_of(to, pair.asset_1);

			let amount_0_in = user_balance_0_pre_deposit - user_balance_0_post_deposit;
			let amount_1_in = user_balance_1_pre_deposit - user_balance_1_post_deposit;
			// both balances should decrease equally because the asset pair is 1:1
			// i.e. the user has to pay an equal amount of each token
			assert_eq!(amount_0_in, amount_1_in);
			assert_eq!(user_balance_0_pre_deposit - deposit_amount, user_balance_0_post_deposit);
			assert_eq!(user_balance_1_pre_deposit - deposit_amount, user_balance_1_post_deposit);

			// check contract balances
			let contract_balance_0_post_deposit = pair.reserve_0;
			let contract_balance_1_post_deposit = pair.reserve_1;
			assert_eq!(contract_balance_0_post_deposit, contract_balance_1_post_deposit);
			assert_eq!(deposit_amount, contract_balance_0_post_deposit);
		}

		#[ink::test]
		#[serial]
		fn deposit_works_for_unbalanced_pair() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let to = AccountId::from(TO_BYTE_ARRAY);
			ink_env::test::set_caller::<ink_env::DefaultEnvironment>(to);

			let mut pair = get_default_pair();
			let initial_supply = 1_000;
			add_supply_for_account(to, initial_supply, &pair);

			// execute initial deposit
			let deposit_amount = 100;
			let result = pair.deposit_asset_1(deposit_amount);
			let gained_lp = result.expect("Could not unwrap gained lp");
			assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

			// swap to make it unbalanced
			pair.swap_asset_2_for_asset_1(10).expect("Swap did not work");

			let user_balance_0_pre_deposit = pair.balance_of(to, pair.asset_0);
			let user_balance_1_pre_deposit = pair.balance_of(to, pair.asset_1);

			// deposit on unbalanced pair
			let result = pair.deposit_asset_1(deposit_amount);
			let gained_lp = result.expect("Could not unwrap gained lp");
			assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

			let user_balance_0_post_deposit = pair.balance_of(to, pair.asset_0);
			let user_balance_1_post_deposit = pair.balance_of(to, pair.asset_1);

			let amount_0_in = user_balance_0_pre_deposit - user_balance_0_post_deposit;
			let amount_1_in = user_balance_1_pre_deposit - user_balance_1_post_deposit;

			assert_eq!(deposit_amount, amount_0_in);
			// expect that amount_0_in is less than amount_1_in because
			// the pair has a ratio of 900:1111 after the swap thus TOKEN_0 is more valuable
			assert_eq!(true, amount_0_in < amount_1_in);
		}

		#[ink::test]
		#[serial]
		fn withdraw_without_lp_fails() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let to = AccountId::from(TO_BYTE_ARRAY);
			ink_env::test::set_caller::<ink_env::DefaultEnvironment>(to);

			let mut pair = get_default_pair();
			let initial_supply = 1_000_000;
			add_supply_for_account(to, initial_supply, &pair);

			let result = pair.withdraw(1);
			assert_eq!(Err(Error::WithdrawWithoutSupply), result);

			let gained_lp = pair.deposit_asset_1(5_000).expect("Could not deposit");
			// try withdrawing more LP than account has
			let result = pair.withdraw(gained_lp + 2);
			assert_eq!(Err(Error::InsufficientBalance), result);
		}

		#[ink::test]
		#[serial]
		fn withdraw_works() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let to = AccountId::from(TO_BYTE_ARRAY);
			ink_env::test::set_caller::<ink_env::DefaultEnvironment>(to);

			let mut pair = get_default_pair();
			let initial_supply = 1_000_000;
			add_supply_for_account(to, initial_supply, &pair);

			let deposit_amount = 50_000;
			let result = pair.deposit_asset_1(deposit_amount);
			let gained_lp = result.expect("Could not unwrap gained lp");
			assert_eq!(gained_lp > 0, true, "Expected received amount of LP to be greater than 0");

			let user_balance_0_pre_withdraw = pair.balance_of(to, pair.asset_0);
			let user_balance_1_pre_withdraw = pair.balance_of(to, pair.asset_1);

			// We cannot withdraw all LP because the pair would be empty so we withdraw with 1 less LP token
			let result = pair.withdraw(gained_lp - 1);
			let (amount_0, amount_1) = result.expect("Could not unwrap result");
			assert_eq!(true, amount_0 > 0, "Expected received amount to be greater than 0");
			assert_eq!(true, amount_1 > 0, "Expected received amount to be greater than 0");

			let user_balance_0_post_withdraw = pair.balance_of(to, pair.asset_0);
			let user_balance_1_post_withdraw = pair.balance_of(to, pair.asset_1);

			assert_eq!(user_balance_0_post_withdraw, user_balance_0_pre_withdraw + amount_0);
			assert_eq!(user_balance_1_post_withdraw, user_balance_1_pre_withdraw + amount_1);
		}

		#[ink::test]
		#[serial]
		fn deposit_and_withdraw_work() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let to = AccountId::from(TO_BYTE_ARRAY);
			ink_env::test::set_caller::<ink_env::DefaultEnvironment>(to);

			let mut pair = get_default_pair();
			let initial_supply = 10_000_000;
			add_supply_for_account(to, initial_supply, &pair);

			let deposit_amount = 500_000;
			// do initial deposit which initiates total_supply
			pair.deposit_asset_1(deposit_amount).expect("Could not deposit");

			// do second deposit
			let result = pair.deposit_asset_1(deposit_amount);
			let gained_lp = result.expect("Could not unwrap gained lp");
			assert_eq!(gained_lp > 0, true, "Expected received amount of LP to be greater than 0");

			let result = pair.withdraw(gained_lp);
			let (amount_0, amount_1) = result.expect("Could not unwrap result");
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
		#[serial]
		fn swap_works_with_small_amount() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let to = AccountId::from(TO_BYTE_ARRAY);
			ink_env::test::set_caller::<ink_env::DefaultEnvironment>(to);

			let mut pair = get_default_pair();
			let initial_supply = 1_000_000;
			add_supply_for_account(to, initial_supply, &pair);

			let gained_lp = pair.deposit_asset_1(500);
			let gained_lp = gained_lp.expect("Could not unwrap gained lp");
			assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

			let swap_amount = 100;
			let user_balance_0_pre_swap = pair.balance_of(to, pair.asset_0);

			let result = pair.swap_asset_2_for_asset_1(swap_amount);
			result.expect("Encountered error in swap");

			let user_balance_0_post_swap = pair.balance_of(to, pair.asset_0);
			assert_eq!(user_balance_0_post_swap, user_balance_0_pre_swap + swap_amount);

			let user_balance_1_pre_swap = pair.balance_of(to, pair.asset_1);

			let result = pair.swap_asset_1_for_asset_2(swap_amount);
			result.expect("Encountered error in swap");

			let user_balance_1_post_swap = pair.balance_of(to, pair.asset_1);
			assert_eq!(user_balance_1_post_swap, user_balance_1_pre_swap + swap_amount);
		}

		#[ink::test]
		#[serial]
		fn swap_works_with_large_amount() {
			reset_map();
			ink_env::test::register_chain_extension(MockedBalanceExtension);
			ink_env::test::register_chain_extension(MockedTransferExtension);

			let to = AccountId::from(TO_BYTE_ARRAY);
			ink_env::test::set_caller::<ink_env::DefaultEnvironment>(to);

			let mut pair = get_default_pair();
			let initial_supply = 10_000_000;
			add_supply_for_account(to, initial_supply, &pair);

			let gained_lp = pair.deposit_asset_1(1_000_000);
			let gained_lp = gained_lp.expect("Could not unwrap gained lp");
			assert_eq!(gained_lp > 0, true, "Expected lp to be greater than 0");

			let swap_amount = 200_000;
			let user_balance_0_pre_swap = pair.balance_of(to, pair.asset_0);

			debug_println!("BALANCES: {:?}", BALANCES.lock().unwrap());
			let result = pair.swap_asset_2_for_asset_1(swap_amount);
			debug_println!("BALANCES post swap: {:?}", BALANCES.lock().unwrap());
			result.expect("Encountered error in swap");

			let user_balance_0_post_swap = pair.balance_of(to, pair.asset_0);
			assert_eq!(user_balance_0_post_swap, user_balance_0_pre_swap + swap_amount);
			let user_balance_1_pre_swap = pair.balance_of(to, pair.asset_1);

			let result = pair.swap_asset_1_for_asset_2(swap_amount);
			result.expect("Encountered error in swap");

			let user_balance_1_post_swap = pair.balance_of(to, pair.asset_1);
			assert_eq!(user_balance_1_post_swap, user_balance_1_pre_swap + swap_amount);
		}
	}
}
