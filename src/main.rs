use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    slackslack::run().await
}
