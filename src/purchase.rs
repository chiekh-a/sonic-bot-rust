use ethers::{
    prelude::*,
    core::types::{Address, U256, BlockNumber},
    providers::Middleware,
};
use std::sync::Arc;
use std::str::FromStr;
use eyre::Result;

const ROUTER_ABI: &str = r#"[
    function swapExactTokensForTokens(
        uint amountIn,
        uint amountOutMin,
        address[] calldata path,
        address to,
        uint deadline
    ) external returns (uint[] memory amounts)
]"#;

struct SwapClient {
    client: Arc<Provider<Http>>,
    wallet: LocalWallet,
    router_address: Address,
    sonic_address: Address,
    token_address: Address,
}

impl SwapClient {
    async fn new(
        rpc_url: &str,
        private_key: &str,
        router_address: &str,
        sonic_address: &str,
        token_address: &str,
    ) -> Result<Self> {
        let provider = Provider::<Http>::try_from(rpc_url)?;
        let client = Arc::new(provider);
        
        let wallet = LocalWallet::from_str(private_key)?
            .with_chain_id(250u64);

        Ok(Self {
            client,
            wallet,
            router_address: Address::from_str(router_address)?,
            sonic_address: Address::from_str(sonic_address)?,
            token_address: Address::from_str(token_address)?,
        })
    }

    async fn execute_swap(&self, amount_in: U256) -> Result<Option<H256>> {
        let router = Contract::new(
            self.router_address,
            ROUTER_ABI.parse()?,
            self.wallet.clone().connect(self.client.clone()),
        );

        let path = vec![self.sonic_address, self.token_address];
        
        // Get current block for gas optimization
        let pending_block = self.client.get_block(BlockNumber::Pending).await?;
        let base_fee = pending_block
            .and_then(|b| b.base_fee_per_gas)
            .unwrap_or_else(|| U256::from(50_000_000_000u64)); // 50 gwei fallback

        // Aggressive gas settings for fast inclusion
        let priority_fee = U256::from(2_000_000_000u64); // 2 gwei priority fee
        let max_fee = base_fee + priority_fee;

        let deadline = U256::from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
                + 15 // Very short deadline
        );

        // Attempt transaction with no slippage
        let tx = router
            .method::<_, Vec<U256>>(
                "swapExactTokensForTokens",
                (
                    amount_in,
                    U256::zero(), // No minimum output (zero slippage)
                    path,
                    self.wallet.address(),
                    deadline,
                ),
            )?
            .gas(U256::from(300000))
            .max_priority_fee_per_gas(priority_fee)
            .max_fee_per_gas(max_fee)
            .send()
            .await;

        match tx {
            Ok(pending_tx) => {
                println!("Transaction sent: {:?}", pending_tx.tx_hash());
                Ok(Some(pending_tx.tx_hash()))
            },
            Err(err) => {
                println!("Transaction failed: {:?}", err);
                Ok(None)
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = SwapClient::new(
        "https://rpc.ftm.tools",
        "your_private_key_here",
        "router_contract_address",
        "sonic_token_address",
        "target_token_address",
    ).await?;

    let amount_in = U256::from(1000000000000000000u64); // 1 token

    // Single attempt to execute the swap
    match client.execute_swap(amount_in).await? {
        Some(tx_hash) => println!("Swap executed successfully. Hash: {:?}", tx_hash),
        None => println!("Swap failed, aborting."),
    }

    Ok(())
}