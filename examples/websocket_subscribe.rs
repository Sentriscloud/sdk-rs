use std::time::Duration;

use sentrix_chain::{
    bft::{Channel, SubscriptionManager},
    Network,
};

fn network_from_env() -> Network {
    match std::env::var("SENTRIX_NETWORK").as_deref() {
        Ok("testnet") => Network::Testnet,
        _ => Network::Mainnet,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = SubscriptionManager::new(network_from_env());
    let mut heads = manager.subscribe(Channel::NewHeads).await?;

    match tokio::time::timeout(Duration::from_secs(30), heads.recv()).await {
        Ok(Some(head)) => println!("new_head={head}"),
        Ok(None) => println!("subscription closed before a head arrived"),
        Err(_) => println!("no head received within 30 seconds"),
    }

    Ok(())
}
