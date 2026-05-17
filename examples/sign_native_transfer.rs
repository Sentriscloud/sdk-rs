use sentrix_chain::{NativeClient, Network, SentrixWallet};

fn network_from_env() -> Network {
    match std::env::var("SENTRIX_NETWORK").as_deref() {
        Ok("testnet") => Network::Testnet,
        _ => Network::Mainnet,
    }
}

fn parse_u64_env(name: &str, default: u64) -> Result<u64, Box<dyn std::error::Error>> {
    match std::env::var(name) {
        Ok(value) => Ok(value.parse()?),
        Err(_) => Ok(default),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let private_key = match std::env::var("SENTRIX_PRIVATE_KEY") {
        Ok(value) => value,
        Err(_) => {
            println!("set SENTRIX_PRIVATE_KEY and SENTRIX_TO to build a signed transfer");
            return Ok(());
        }
    };
    let to = match std::env::var("SENTRIX_TO") {
        Ok(value) => value,
        Err(_) => {
            println!("set SENTRIX_TO to the 0x recipient address");
            return Ok(());
        }
    };

    let network = network_from_env();
    let client = NativeClient::new(network);
    let wallet = SentrixWallet::from_private_key_hex(&private_key)?;
    let nonce = match std::env::var("SENTRIX_NONCE") {
        Ok(value) => value.parse()?,
        Err(_) => client.next_nonce(&wallet.address).await?,
    };

    let tx = wallet.build_and_sign_transfer(
        &to,
        parse_u64_env("SENTRIX_AMOUNT_SENTRI", 100_000_000)?,
        parse_u64_env("SENTRIX_FEE_SENTRI", 10_000)?,
        nonce,
        client.spec().chain_id,
    )?;

    println!("{}", serde_json::to_string_pretty(&tx)?);
    println!("not broadcast; submit with NativeClient::broadcast after review");

    Ok(())
}
