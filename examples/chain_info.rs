use sentrix_chain::{NativeClient, Network};

fn network_from_env() -> Network {
    match std::env::var("SENTRIX_NETWORK").as_deref() {
        Ok("testnet") => Network::Testnet,
        _ => Network::Mainnet,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = network_from_env();
    let client = NativeClient::new(network);
    let info = client.chain_info().await?;

    println!("network={}", client.spec().name);
    println!("chain_id={}", client.spec().chain_id);
    println!("height={}", info.height);
    println!("active_validators={}", info.active_validators);
    println!("mempool_size={}", info.mempool_size);
    println!("total_minted_srx={}", info.total_minted_srx);
    println!("total_burned_srx={}", info.total_burned_srx);

    Ok(())
}
