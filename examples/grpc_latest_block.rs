use sentrix_chain::{grpc::SentrixGrpcClient, Network};

fn network_from_env() -> Network {
    match std::env::var("SENTRIX_NETWORK").as_deref() {
        Ok("testnet") => Network::Testnet,
        _ => Network::Mainnet,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = network_from_env();
    let mut client = SentrixGrpcClient::connect(network).await?;
    let block = client.get_latest_block().await?;

    println!("latest_block_height={}", block.index);
    println!("transactions={}", block.transactions.len());

    Ok(())
}
