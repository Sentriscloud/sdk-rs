use alloy::providers::Provider;
use sentrix_chain::{evm, Network};

fn network_from_env() -> Network {
    match std::env::var("SENTRIX_NETWORK").as_deref() {
        Ok("testnet") => Network::Testnet,
        _ => Network::Mainnet,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = network_from_env();
    let provider = evm::http_provider(network)?;
    let block_number = provider.get_block_number().await?;

    println!("latest_evm_block={block_number}");

    Ok(())
}
