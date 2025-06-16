// Copyright 2025 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use alloy::{
    network::{Network, TransactionBuilder},
    primitives::Address,
    providers::{
        fillers::{FillerControlFlow, GasFillable, GasFiller, TxFiller},
        Provider, SendableTx,
    },
    transports::TransportResult,
};

#[derive(Clone, Copy, Debug)]
/// A gas filler that dynamically adjusts the gas price based on the number of pending transactions.
///
/// This filler increases the gas price by a factor of `gas_increase_factor` for each pending transaction
/// up to a maximum of `max_gas_multiplier`.
pub struct DynamicGasFiller {
    /// The factor by which to increase the gas limit.
    pub gas_limit_factor: f64,
    /// The factor by which to increase the gas price for each pending transaction.
    pub gas_increase_factor: f64,
    /// The maximum gas price multiplier.
    pub max_gas_multiplier: f64,
    /// The address to check the pending transaction count for.
    pub address: Address,
}

impl DynamicGasFiller {
    /// Creates a new `DynamicMempoolGasFiller`.
    ///
    /// # Arguments
    ///
    /// * `gas_limit_factor` - The factor by which to increase the gas limit.
    /// * `gas_increase_factor` - The factor by which to increase the gas price for each pending transaction.
    /// * `max_gas_multiplier` - The maximum gas price multiplier.
    /// * `address` - The address to check the pending transaction count for.
    pub fn new(
        gas_limit_factor: f64,
        gas_increase_factor: f64,
        max_gas_multiplier: f64,
        address: Address,
    ) -> Self {
        Self { gas_limit_factor, gas_increase_factor, max_gas_multiplier, address }
    }
}

/// Parameters for the dynamic gas filler.
pub struct DynamicGasParams {
    /// The fillable gas parameters.
    pub fillable: GasFillable,
    /// The multiplier to apply to the gas price.
    pub multiplier: f64,
}

impl<N: Network> TxFiller<N> for DynamicGasFiller {
    type Fillable = DynamicGasParams;

    fn status(&self, tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        TxFiller::<N>::status(&GasFiller, tx)
    }

    fn fill_sync(&self, _tx: &mut SendableTx<N>) {}

    async fn prepare<P>(
        &self,
        provider: &P,
        tx: &N::TransactionRequest,
    ) -> TransportResult<Self::Fillable>
    where
        P: Provider<N>,
    {
        let fillable = GasFiller.prepare(provider, tx).await?;

        let confirmed_nonce = provider.get_transaction_count(self.address).latest().await?;
        let pending_nonce = provider.get_transaction_count(self.address).pending().await?;

        let tx_diff = pending_nonce.saturating_sub(confirmed_nonce) as u128;
        tracing::debug!(
            "DynamicGasFiller: Pending transactions: {}, confirmed transactions: {} - tx_diff: {}",
            pending_nonce,
            confirmed_nonce,
            tx_diff
        );
        let mut multiplier = 1.0 + (tx_diff as f64 * self.gas_increase_factor);
        multiplier = multiplier.min(self.max_gas_multiplier);

        Ok(DynamicGasParams { fillable, multiplier })
    }

    async fn fill(
        &self,
        params: Self::Fillable,
        mut tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        if let Some(builder) = tx.as_mut_builder() {
            match params.fillable {
                GasFillable::Legacy { gas_limit, .. } => {
                    let adjusted_gas_limit = (gas_limit as f64 * (1.0 + self.gas_limit_factor)).ceil() as u64;
                    builder.set_gas_limit(adjusted_gas_limit);
                    builder.set_gas_price(30_000_000_000u128); // 
                }
                GasFillable::Eip1559 { gas_limit, .. } => {
                    let adjusted_gas_limit = (gas_limit as f64 * (1.0 + self.gas_limit_factor)).ceil() as u64;
                    builder.set_gas_limit(adjusted_gas_limit);
                    builder.set_max_fee_per_gas(30_000_000_000u128);         // 
                    builder.set_max_priority_fee_per_gas(30_000_000_000u128); // 
                }
            }
        }

        Ok(tx)
    }
}
