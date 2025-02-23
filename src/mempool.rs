use web3::{
    Web3,
    transports::WebSocket,
    types::{TransactionId, H256},
    futures::StreamExt,
};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashSet;

pub async fn stream_mempool(
    web3: Web3<WebSocket>,
    mempool_txs: Arc<Mutex<HashSet<H256>>>,
) -> web3::Result<()> {
    let mut pending_tx_stream = web3.eth_subscribe().subscribe_new_pending_transactions().await?;
    println!("Mempool subscription started at {}", Utc::now().to_rfc3339());

    while let Some(tx_hash) = pending_tx_stream.next().await {
        let received_time = Utc::now();
        match tx_hash {
            Ok(hash) => {
                // Get full transaction details
                if let Ok(Some(tx)) = web3.eth().transaction(TransactionId::Hash(hash)).await {
                    // Get receipt and only process if it's a contract creation
                    if let Ok(Some(receipt)) = web3.eth().transaction_receipt(tx.hash).await {
                        if let Some(contract_address) = receipt.contract_address {
                            let processed_time = Utc::now();
                            {
                                let mut mempool = mempool_txs.lock().await;
                                mempool.insert(tx.hash);
                                println!("Current mempool size: {}", mempool.len());
                            }

                            println!("\n🔄 PENDING CONTRACT CREATION DETECTED 🔄");
                            println!("Creator: {:?}", tx.from);
                            println!("Contract Address: {:?}", contract_address);
                            println!("Transaction Hash: {:?}", tx.hash);
                            println!("Gas Price: {:?}", tx.gas_price);
                            println!("Hash received at: {}", received_time.to_rfc3339());
                            println!("Transaction details fetched at: {}", processed_time.to_rfc3339());
                            println!("Processing delay: {} ms", 
                                (processed_time - received_time).num_milliseconds());
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Mempool subscription error: {}", e);
                break;
            }
        }
    }

    Ok(())
}