use alloy::{
    eips::BlockId,
    providers::{Provider, ProviderBuilder},
    rpc::client::ClientBuilder,
};
use alloy_throttle::ThrottleLayer;
use alloy_transport::layers::RetryBackoffLayer;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let rpc_endpoint = std::env::var("ETHEREUM_PROVIDER")?;

    let client = ClientBuilder::default()
        .layer(ThrottleLayer::new(40, None)?)
        // The RetryBackoffLayer can be stacked with the throttle layer to retry failed requests
        .layer(RetryBackoffLayer::new(10, 300, 330))
        .http(rpc_endpoint.parse()?);

    let provider = ProviderBuilder::new().on_client(client);

    let block_number = provider.get_block_number().await?;
    for _ in block_number - 100..block_number {
        let block = provider.get_block(BlockId::latest()).await?;

        if let Some(block) = block {
            println!("Tx count: {:?}", block.transactions.len());
        }
    }

    Ok(())
}
