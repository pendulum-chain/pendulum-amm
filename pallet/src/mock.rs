use std::cell::RefCell;
use std::collections::HashMap;
use frame_support::{parameter_types, sp_io};
use frame_support::pallet_prelude::GenesisBuild;
use frame_support::traits::{ConstU16, ConstU64, ConstU128};
use frame_system as system;
use sp_runtime::{
    testing::Header,
    traits::{BlakeTwo256, IdentityLookup},
};

use crate as amm;
use sp_runtime::app_crypto::sp_core;
use amm::{pallet::Config, AmmExtension};

type UncheckedExtrinsic = system::mocking::MockUncheckedExtrinsic<Test>;
type Block = system::mocking::MockBlock<Test>;
type AccountId = u64;
type Balance = u128;
type Moment = u64;


pub type AssetCode = [u8; 12];
pub type IssuerId = [u8; 32];

#[derive(Debug, Clone, Copy, Ord, PartialOrd, codec::Encode, codec::Decode, Eq, PartialEq, Default, codec::MaxEncodedLen, scale_info::TypeInfo, serde::Serialize, serde::Deserialize)]
pub struct Asset {
    code: AssetCode,
    issuer: IssuerId
}


const MILLISECS_PER_BLOCK: u64 = 6000;
const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

const EUR :[u8;12] =  [69, 85, 82, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const USDC :[u8;12] = [85, 83, 68, 67, 0, 0, 0, 0, 0, 0, 0, 0];

const ISSUER: [u8; 32] = [
    20, 209, 150, 49, 176, 55, 23, 217, 171, 154, 54, 110, 16, 50, 30, 226, 102, 231, 46, 199, 108,
    171, 97, 144, 240, 161, 51, 109, 72, 34, 159, 139,
];

const ASSET_0: Asset = Asset{
    code: EUR,
    issuer: ISSUER
};

const ASSET_1: Asset = Asset{
    code: USDC,
    issuer: ISSUER
};

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: system::{Pallet, Call, Config, Storage, Event<T>},
        Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},

        Amm: amm::{Pallet, Call, Storage, Event<T>}
	}
);

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type Origin = Origin;
    type Call = Call;
    type Index = u64;
    type BlockNumber = u64;
    type Hash = sp_core::H256;
    type Hashing = BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Header = Header;
    type Event = Event;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = ();
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Test {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = ();
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

impl Config for Test {
    type Event = Event;
    type Balance = Balance;
    type CurrencyId = Asset;
    type AmmExtension = Extension;
    // type AddressConversion = ();
    type MinimumLiquidity = ConstU128<1>;
    type MintFee = ConstU128<5>;
    type SubFee = ConstU128<997>;
    type MulBalance = ConstU128<1000>;
    type SwapMulBalance = ConstU128<3>;
}

thread_local! {
    pub static ASSETSMAP0: RefCell<HashMap<AccountId,Balance>> = RefCell::new(HashMap::new());
    pub static ASSETSMAP1: RefCell<HashMap<AccountId,Balance>> = RefCell::new(HashMap::new());
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    ASSETSMAP0.with(|assets|{
        let mut assets_map = assets.borrow_mut();
        assets_map.insert(1,1000);
        assets_map.insert(2, 200);
        assets_map.insert(3, 300);
        assets_map.insert(4, 400);
    });

    ASSETSMAP1.with(|assets|{
        let mut assets_map = assets.borrow_mut();
        assets_map.insert(1,900);
        assets_map.insert(2, 201);
        assets_map.insert(5, 500);
        assets_map.insert(6, 600);
        assets_map.insert(7, 700);
    });

    let mut system_cfg = system::GenesisConfig::default().build_storage::<Test>().unwrap();

    amm::GenesisConfig::<Test> {
        contract_id: Some(1),
        zero_account: Some(0),
        fee_to_setter: Some(2),
        asset_0: Some(ASSET_0),
        asset_1: Some(ASSET_1)
    }
    .assimilate_storage(&mut system_cfg)
    .unwrap();

    system_cfg.into()

}

pub struct Extension;

impl AmmExtension<AccountId,Asset,Balance,Moment> for Extension {
    fn fetch_balance(owner: &AccountId, asset: Asset) -> Balance {
        if asset == ASSET_0 {
            ASSETSMAP0.with(|assets| {
                let asset_map = assets.borrow();
                *asset_map.get(owner).unwrap_or(&0)
            })
        } else {
            ASSETSMAP1.with(|assets| {
                let asset_map = assets.borrow();
                *asset_map.get(owner).unwrap_or(&0)
            })
        }
    }

    fn transfer_balance(from: &AccountId, to: &AccountId, asset: Asset, amount: Balance) {
        if asset == ASSET_0 {
            ASSETSMAP0.with(|assets| {
                let mut asset_map = assets.borrow_mut();

                if let Some(bal) = asset_map.get(from) {
                    let new_bal = bal.checked_sub(amount).unwrap_or(0u128);

                    asset_map.insert(to.clone(),amount);
                    asset_map.insert(from.clone(),new_bal);
                }
            })
        } else {
            ASSETSMAP1.with(|assets| {
                let mut asset_map = assets.borrow_mut();

                if let Some(bal) = asset_map.get(from) {
                    let new_bal = bal.checked_sub(amount).unwrap_or(0u128);

                    asset_map.insert(to.clone(),amount);
                    asset_map.insert(from.clone(),new_bal);
                }
            })
        }
    }

    fn moment_to_balance_type(moment: Moment) -> Balance {
        moment as Balance
    }
}

