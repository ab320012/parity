// Copyright 2015-2018 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Hybrid Casper related functionalities.

use bytes::Bytes;
use ethereum_types::{Address, U256, H256};
use engines::{DEFAULT_CASPER_CONTRACT, DEFAULT_PURITY_CHECKER_CONTRACT, DEFAULT_MSG_HASHER_CONTRACT, DEFAULT_RLP_DECODER_CONTRACT};
use rustc_hex::FromHex;
use transaction::{SignedTransaction, Action};
use vm::Schedule;
use state::{State, Backend};
use types::BlockNumber;
use ethabi::{self, ParamType};
use super::SystemCall;

use_contract!(simple_casper, "SimpleCasper", "res/contracts/simple_casper.json");

/// Hybrid Casper parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridCasperParams {
	/// Main contract code.
	pub contract_code: Bytes,
	/// Address to deploy the main contract.
	pub contract_address: Address,
	/// Balance to force set in the beginning.
	pub contract_balance: U256,
	/// Purity checker contract code.
	pub purity_checker_contract_code: Bytes,
	/// Address to deploy the purity checker.
	pub purity_checker_contract_address: Address,
	/// Msg hasher contract code.
	pub msg_hasher_contract_code: Bytes,
	/// Address to deploy the msg hasher.
	pub msg_hasher_contract_address: Address,
	/// RLP decoder contract code.
	pub rlp_decoder_contract_code: Bytes,
	/// Address to deploy the RLP decoder.
	pub rlp_decoder_contract_address: Address,
	/// Whether force-deploying the RLP decoder or not.
	pub deploy_rlp_decoder: bool,

	/// Casper epoch length.
	pub epoch_length: u64,
	/// Casper withdrawal delay.
	pub withdrawal_delay: u64,
	/// Casper dynasty logout delay.
	pub dynasty_logout_delay: u64,
	/// Base interest factor passed to the Casper init function.
	pub base_interest_factor: U256,
	/// Base penalty factor passed to the Casper init function.
	pub base_penalty_factor: U256,
	/// Min deposit size accepted by Casper.
	pub min_deposit_size: U256,
	/// Warm up period before vote begins.
	pub warm_up_period: u64,
	/// Min deposit to consider a block to be justified.
	pub non_revert_min_deposits: U256,
}

impl From<::ethjson::spec::HybridCasperParams> for HybridCasperParams {
	fn from(p: ::ethjson::spec::HybridCasperParams) -> Self {
		let rlp_decoder_contract_address = p.rlp_decoder_contract_address.map_or(Address::from(0x43u64), Into::into);

		HybridCasperParams {
			contract_code: p.contract_code
				.map_or(DEFAULT_CASPER_CONTRACT
						.replace("<rlp_decoder>", &format!("{:x}", rlp_decoder_contract_address))
						.from_hex()
						.expect("DEFAULT_CASPER_CONTRACT is valid bytearray; qed"), Into::into),
			contract_address: p.contract_address.map_or(Address::from(0x40u64), Into::into),
			contract_balance: p.contract_balance.map_or(U256::from(1250000) * ::ethereum::ether(), Into::into),

			purity_checker_contract_code: p.purity_checker_contract_code
				.map_or(DEFAULT_PURITY_CHECKER_CONTRACT
						.from_hex()
						.expect("DEFAULT_PURITY_CHECKER_CONTRACT is valid bytearray; qed"), Into::into),
			purity_checker_contract_address: p.purity_checker_contract_address.map_or(Address::from(0x41u64), Into::into),

			msg_hasher_contract_code: p.msg_hasher_contract_code
				.map_or(DEFAULT_MSG_HASHER_CONTRACT
						.from_hex()
						.expect("DEFAULT_MSG_HASHER_CONTRACT is valid bytearray; qed"), Into::into),
			msg_hasher_contract_address: p.msg_hasher_contract_address.map_or(Address::from(0x42u64), Into::into),

			rlp_decoder_contract_code: p.rlp_decoder_contract_code
				.map_or(DEFAULT_RLP_DECODER_CONTRACT
						.from_hex()
						.expect("DEFAULT_RLP_DECODER_CONTRACT is valid bytearray; qed"), Into::into),
			rlp_decoder_contract_address: rlp_decoder_contract_address,
			deploy_rlp_decoder: p.deploy_rlp_decoder.unwrap_or(true),

			epoch_length: p.epoch_length.map_or(5, Into::into),
			withdrawal_delay: p.withdrawal_delay.map_or(150, Into::into),
			dynasty_logout_delay: p.dynasty_logout_delay.map_or(70, Into::into),
			base_interest_factor: p.base_interest_factor.map_or(U256::from(70000000), Into::into),
			base_penalty_factor: p.base_penalty_factor.map_or(U256::from(2000), Into::into),
			min_deposit_size: p.min_deposit_size.map_or(U256::from(5) * ::ethereum::ether(), Into::into),
			warm_up_period: p.warm_up_period.map_or(5, Into::into),
			non_revert_min_deposits: p.non_revert_min_deposits.map_or(U256::from(1) * ::ethereum::ether(), Into::into),
		}
	}
}

impl Default for HybridCasperParams {
	fn default() -> Self {
		Self::from(::ethjson::spec::HybridCasperParams::default())
	}
}

/// Casper related metadata.
#[derive(Debug, PartialEq, Clone, RlpEncodable, RlpDecodable)]
pub struct HybridCasperMetadata {
	/// Gas used in vote transactions.
	pub vote_gas_used: U256,
	/// Highest justified epoch returned by Casper contract.
	pub highest_justified_epoch: U256,
	/// Highest finalized epoch returned by Casper contract.
	pub highest_finalized_epoch: U256,
	/// Highest finalized block hash returned by Casper contract.
	pub highest_finalized_hash: H256,
}

impl Default for HybridCasperMetadata {
	fn default() -> Self {
		Self {
			vote_gas_used: U256::zero(),
			highest_justified_epoch: U256::zero(),
			highest_finalized_epoch: U256::zero(),
			highest_finalized_hash: Default::default(),
		}
	}
}

pub struct HybridCasper {
	params: HybridCasperParams,
	provider: simple_casper::SimpleCasper,
}

impl HybridCasper {
	pub fn new(params: HybridCasperParams) -> Self {
		Self {
			params,
			provider: simple_casper::SimpleCasper::default(),
		}
	}

	pub fn is_vote_transaction(&self, transaction: &SignedTransaction) -> bool {
		if !transaction.is_unsigned() {
			return false;
		}

		let unsigned = transaction.as_unsigned();

		match unsigned.action {
			Action::Create => {
				return false;
			},
			Action::Call(address) => {
				if address != self.params.contract_address {
					return false;
				}
			},
		}

		if unsigned.data.len() < 4 {
			return false;
		}

		if &unsigned.data[0..4] != &[0xe9, 0xdc, 0x06, 0x14] {
			return false;
		}

		return true;
	}

	pub fn enable_casper_schedule(&self, schedule: &mut Schedule) {
		schedule.eip86 = true;
	}

	pub fn init_state<B: Backend>(&self, state: &mut State<B>) -> Result<(), ::error::Error> {
		state.new_contract(&self.params.contract_address,
						   self.params.contract_balance,
						   U256::zero());
		state.init_code(&self.params.contract_address,
						self.params.contract_code.clone())?;
		state.init_code(&self.params.purity_checker_contract_address,
						self.params.purity_checker_contract_code.clone())?;
		state.init_code(&self.params.msg_hasher_contract_address,
						self.params.msg_hasher_contract_code.clone())?;
		if self.params.deploy_rlp_decoder {
			state.init_code(&self.params.rlp_decoder_contract_address,
							self.params.rlp_decoder_contract_code.clone())?;
		}

		Ok(())
	}

	pub fn init_casper_contract(&self, caller: &mut SystemCall) -> Result<(), ::error::Error> {
		let data = self.provider.functions().init().input(
			self.params.epoch_length,
			self.params.warm_up_period,
			self.params.withdrawal_delay,
			self.params.dynasty_logout_delay,
			self.params.msg_hasher_contract_address,
			self.params.purity_checker_contract_address,
			self.params.base_interest_factor,
			self.params.base_penalty_factor,
			self.params.min_deposit_size,
		);
		caller(self.params.contract_address, data)
			.map(|_| ())
			.map_err(::engines::EngineError::FailedSystemCall)
			.map_err(Into::into)
	}

	pub fn on_new_epoch(&self, block_number: BlockNumber, caller: &mut SystemCall) -> Result<(), ::error::Error> {
		if block_number % self.params.epoch_length == 0 {
			let data = self.provider.functions().initialize_epoch().input(
				block_number / self.params.epoch_length
			);
			caller(self.params.contract_address, data)
				.map(|_| ())
				.map_err(::engines::EngineError::FailedSystemCall)
				.map_err(Into::into)
		} else {
			Ok(())
		}
	}

	pub fn highest_justified_epoch(&self, caller: &mut SystemCall) -> Result<U256, ::error::Error> {
		let data = self.provider.functions().highest_justified_epoch().input(
			self.params.non_revert_min_deposits,
		);
		caller(self.params.contract_address, data)
			.and_then(|output| {
				Ok(ethabi::decode(&[ParamType::Int(128)], &output)
				   .map_err(|e| format!("{}", e))?[0].clone()
				   .to_int()
				   .expect("type checked by ethabi::decode; qed"))
			})
			.map_err(::engines::EngineError::FailedSystemCall)
			.map_err(Into::into)
	}

	pub fn highest_finalized_epoch(&self, caller: &mut SystemCall) -> Result<U256, ::error::Error> {
		let data = self.provider.functions().highest_finalized_epoch().input(
			self.params.non_revert_min_deposits,
		);
		caller(self.params.contract_address, data)
			.and_then(|output| {
				Ok(ethabi::decode(&[ParamType::Int(128)], &output)
				   .map_err(|e| format!("{}", e))?[0].clone()
				   .to_int()
				   .expect("type checked by ethabi::decode; qed"))
			})
			.map_err(::engines::EngineError::FailedSystemCall)
			.map_err(Into::into)
	}

	pub fn checkpoint_hashes(&self, epoch: U256, caller: &mut SystemCall) -> Result<H256, ::error::Error> {
		let data = self.provider.functions().checkpoint_hashes().input(
			epoch,
		);
		caller(self.params.contract_address, data)
			.and_then(|output| {
				Ok(H256::from_slice(&ethabi::decode(&[ParamType::FixedBytes(32)], &output)
									.map_err(|e| format!("{}", e))?[0].clone()
									.to_fixed_bytes()
									.expect("type checked by ethabi::decode; qed")))
			})
			.map_err(::engines::EngineError::FailedSystemCall)
			.map_err(Into::into)
	}

	pub fn update_metadata(&self, metadata: &mut HybridCasperMetadata, caller: &mut SystemCall) -> Result<(), ::error::Error> {
		metadata.highest_justified_epoch = self.highest_justified_epoch(caller)?;
		metadata.highest_finalized_epoch = self.highest_finalized_epoch(caller)?;
		metadata.highest_finalized_hash = self.checkpoint_hashes(metadata.highest_finalized_epoch, caller)?;

		Ok(())
	}
}