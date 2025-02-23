use std::{time::Duration, thread};
use web3::{
    Web3,
    transports::WebSocket,
    types::{BlockHeader, Transaction, TransactionReceipt, BlockNumber, BlockId},
    futures::{StreamExt},
};
use tokio;
use chrono::{Utc};
use futures;

const WS_URL: &str = "wss://rpc.soniclabs.com";

async fn test_connection(web3: &Web3<WebSocket>) -> web3::Result<()> {
    // Get current block number
    let block_number = web3.eth().block_number().await?;
    println!("Connected! Current block number: {}", block_number);

    // Get chain ID
    let chain_id = web3.eth().chain_id().await?;
    println!("Chain ID: {}", chain_id);

    Ok(())
}

async fn process_transaction(
    tx: Transaction,
    receipt: TransactionReceipt,
) {
    // If transaction has no 'to' address, it's a contract creation
    if receipt.contract_address.is_some() {
        println!("\n🚨 CONTRACT CREATION DETECTED 🚨");
        println!("Creator: {:?}", tx.from);
        println!("Contract Address: {:?}", receipt.contract_address);
        println!("Transaction Hash: {:?}", tx.hash);
        println!("Block Number: {}", receipt.block_number.unwrap());
        println!("Processed at: {}", Utc::now().to_rfc3339());

        // If there are any logs, show them
        if !receipt.logs.is_empty() {
            println!("Event Logs: {:?}", receipt.logs);
        }
    }
}

async fn stream_events(web3: Web3<WebSocket>) -> web3::Result<()> {
    let mut block_stream = web3.eth_subscribe().subscribe_new_heads().await?;
    println!("Subscription started");

    // Create a mpsc channel with larger buffer for more parallelism
    let (tx, mut rx) = tokio::sync::mpsc::channel(1000); 
    
    // Spawn a single receiver task that processes blocks in batches
    let distributor_handle = tokio::spawn(async move {
        let mut futures = Vec::new();
        const BATCH_SIZE: usize = 10; // Process 10 blocks at a time

        while let Some(header) = rx.recv().await {
            let web3_clone = web3.clone();
            let future = tokio::spawn(async move {
                if let Err(e) = process_block(&web3_clone, header).await {
                    eprintln!("Error processing block: {}", e);
                }
            });
            futures.push(future);

            // When we have enough futures or no more blocks in the channel
            if futures.len() >= BATCH_SIZE || rx.try_recv().is_err() {
                // Process all blocks in parallel
                futures::future::join_all(futures).await;
                futures = Vec::new();
            }
        }

        // Process any remaining futures
        if !futures.is_empty() {
            futures::future::join_all(futures).await;
        }
    });

    // Feed blocks to the channel
    while let Some(block_header) = block_stream.next().await {
        match block_header {
            Ok(header) => {
                if let Err(e) = tx.send(header).await {
                    eprintln!("Error sending block to processor: {}", e);
                    break;
                }
            }
            Err(e) => {
                eprintln!("Subscription error: {}", e);
                break;
            }
        }
    }

    // Clean up
    drop(tx); // Signal workers to shut down
    distributor_handle.await.unwrap();

    Ok(())
}

async fn process_block(web3: &Web3<WebSocket>, header: BlockHeader) -> web3::Result<()> {
    let block_number = header.number.unwrap();
    
    // Calculate block time difference
    let block_timestamp = header.timestamp.as_u64() * 1000;
    let current_timestamp = Utc::now().timestamp_millis() as u64;
    let time_diff = current_timestamp.saturating_sub(block_timestamp);
    
    println!(
        "Block {} | Time difference: {} ms", 
        block_number, time_diff
    );
    
    let block = web3
        .eth()
        .block_with_txs(BlockId::Number(BlockNumber::Number(block_number)))
        .await?;
    
    if let Some(block) = block {
        // Filter contract creation transactions first
        let contract_txs: Vec<_> = block.transactions
            .into_iter()
            .filter(|tx| tx.to.is_none())
            .collect();

        if !contract_txs.is_empty() {
            // Batch process all receipts in chunks
            const BATCH_SIZE: usize = 20;
            for chunk in contract_txs.chunks(BATCH_SIZE) {
                let mut receipt_futures = Vec::new();
                
                for tx in chunk {
                    let web3_clone = web3.clone();
                    let tx_clone = tx.clone();
                    let receipt_future = async move {
                        let receipt = web3_clone.eth().transaction_receipt(tx_clone.hash).await?;
                        Ok::<(Transaction, Option<TransactionReceipt>), web3::Error>((tx_clone, receipt))
                    };
                    receipt_futures.push(receipt_future);
                }

                // Process batch of receipts concurrently
                let results = futures::future::join_all(receipt_futures).await;
                for result in results {
                    if let Ok((tx, Some(receipt))) = result {
                        process_transaction(tx, receipt).await;
                    }
                }
            }
        }
    }

    Ok(())
}


#[tokio::main]
async fn main() -> web3::Result<()> {
    loop {
        println!("\nAttempting to establish WebSocket connection...");
        
        // Create new WebSocket connection
        let ws = match WebSocket::new(WS_URL).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("WebSocket connection failed: {} at {}", e, Utc::now().to_rfc3339());
                println!("Retrying connection in 10 seconds...");
                thread::sleep(Duration::from_secs(10));
                continue;
            }
        };
        
        let web3 = Web3::new(ws);

        // Test connection
        if let Err(e) = test_connection(&web3).await {
            eprintln!("Connection test failed: {} at {}", e, Utc::now().to_rfc3339());
            println!("Retrying connection in 10 seconds...");
            thread::sleep(Duration::from_secs(10));
            continue;
        }

        // Stream events
        match stream_events(web3).await {
            Ok(_) => {
                println!("Stream ended normally (this should not happen). Restarting...");
            }
            Err(e) => {
                eprintln!("Stream error: {} at {}", e, Utc::now().to_rfc3339());
                println!("Restarting connection in 10 seconds...");
            }
        }
        
        thread::sleep(Duration::from_secs(10));
    }
}