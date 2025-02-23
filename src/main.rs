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

const WS_URL: &str = "wss://rpc.soniclabs.com";
const CHANNEL_BUFFER_SIZE: usize = 1000;
const MAX_CONCURRENT_RECEIPTS: usize = 100;

async fn create_websocket() -> web3::Result<WebSocket> {
    let ws = WebSocket::new(WS_URL).await?;
    Ok(ws)
}

async fn process_block(web3: &Web3<WebSocket>, header: BlockHeader) -> web3::Result<()> {
    // Get block number early to minimize latency
    let block_number = header.number.unwrap();
    let now = Utc::now().timestamp_millis() as u64;
    
    // Start fetching block data immediately
    let block_future = web3.eth().block_with_txs(BlockId::Number(BlockNumber::Number(block_number)));
    
    // Process the block as soon as we get it
    if let Ok(Some(block)) = block_future.await {
        let block_timestamp_ms = block.timestamp.as_u64() * 1000;
        
        let (tx, mut rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        
        // Spawn a separate task for processing results
        let process_handle = tokio::spawn(async move {
            while let Some((addr, timestamp, now, latency)) = rx.recv().await {
                println!("CONTRACT|{}|{}|{}|{}", addr, timestamp, now, latency);
            }
        });

        // Process transactions in parallel with maximum efficiency
        futures::stream::iter(block.transactions)
            .map(|transaction| {
                let web3 = web3.clone();
                let sender = tx.clone();  // renamed from tx_sender to avoid confusion
                async move {
                    if let Ok(Some(receipt)) = web3.eth().transaction_receipt(transaction.hash).await {
                        if let Some(contract_addr) = receipt.contract_address {
                            let current_time = Utc::now().timestamp_millis() as u64;
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
            .buffer_unordered(MAX_CONCURRENT_RECEIPTS)
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