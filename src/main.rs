use std::time::Duration;
use web3::{
    Web3,
    transports::WebSocket,
    types::{BlockHeader, BlockNumber, BlockId},
    futures::{StreamExt},
};
use tokio;
use chrono::Utc;
use futures;
use tokio::sync::mpsc;

const WS_URL: &str = "wss://rpc.ankr.com/sonic_mainnet/ws/731754ca3f4f0ba8bf96e438ab39ff64dd9633475b5afe3c514d76aaad219784";

async fn create_websocket() -> web3::Result<WebSocket> {
    let ws = WebSocket::new(WS_URL).await?;
    Ok(ws)
}

async fn process_block(web3: &Web3<WebSocket>, header: BlockHeader) -> web3::Result<()> {
    let current_time = Utc::now().timestamp_millis() as u64;
    let block_number = header.number.unwrap();
    
    // Use a bounded channel with a smaller buffer for better backpressure
    let (tx, mut rx) = mpsc::channel(100);
    
    // Spawn the output processor first
    let process_handle = tokio::spawn(async move {
        let mut outputs = Vec::with_capacity(32);
        while let Some((addr, timestamp, now, latency)) = rx.recv().await {
            outputs.push(format!("CONTRACT|{}|{}|{}|{}", addr, timestamp, now, latency));
            // Batch print if we have enough outputs or channel is empty
            if outputs.len() >= 32 || rx.try_recv().is_err() {
                println!("{}", outputs.join("\n"));
                outputs.clear();
            }
        }
        // Print any remaining outputs
        if !outputs.is_empty() {
            println!("{}", outputs.join("\n"));
        }
    });

    // Start fetching block data
    let block = web3.eth().block_with_txs(BlockId::Number(BlockNumber::Number(block_number))).await?;
    
    if let Some(block) = block {
        let block_timestamp_ms = block.timestamp.as_u64() * 1000;
        
        // Process transactions with increased parallelism
        futures::stream::iter(block.transactions)
            .map(|transaction| {
                let web3 = web3.clone();
                let sender = tx.clone();
                async move {
                    let receipt_future = web3.eth().transaction_receipt(transaction.hash);
                    if let Ok(Some(receipt)) = receipt_future.await {
                        if let Some(contract_addr) = receipt.contract_address {
                            let _ = sender.send((
                                contract_addr,
                                block_timestamp_ms,
                                current_time,
                                current_time.saturating_sub(block_timestamp_ms)
                            )).await;
                        }
                    }
                }
            })
            .buffer_unordered(200) // Increased concurrent processing
            .collect::<Vec<()>>()
            .await;

        drop(tx);
        let _ = process_handle.await;
    }

    Ok(())
}

async fn stream_blocks(web3: Web3<WebSocket>) -> web3::Result<()> {
    let mut block_stream = web3.eth_subscribe().subscribe_new_heads().await?;
    
    while let Some(block_header) = block_stream.next().await {
        match block_header {
            Ok(header) => {
                // Spawn a new task for each block to prevent head-of-line blocking
                let web3_clone = web3.clone();
                tokio::spawn(async move {
                    if let Err(e) = process_block(&web3_clone, header).await {
                        eprintln!("Block processing error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Subscription error: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}

#[tokio::main(worker_threads = 16)]
async fn main() -> web3::Result<()> {
    loop {
        match create_websocket().await {
            Ok(ws) => {
                let web3 = Web3::new(ws);
                if let Err(e) = stream_blocks(web3).await {
                    eprintln!("Stream error: {}", e);
                }
            }
            Err(e) => eprintln!("WebSocket connection error: {}", e),
        }
        
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}