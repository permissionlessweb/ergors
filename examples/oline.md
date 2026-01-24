# Oline

deploy and manage a fiull cosmos-sdk sentry-0array full node architechture automatically, thought the inference provider: 


```rs

#[tokio::main]
async fn main() -> Result<()> {
    println!("[INFO] Starting Akash deployment process...");

    // Initialize config
    let config = AkashConfig::mainnet_defaults().await?;
    let mut client = AkashClient::new(config).await?;

    // 1. Setup
    client.setup_keys().await?;
    client.setup_certificate().await?;
    client.check_existing_deployments().await?;

    // SDL files (adjust paths as needed)
    let sdl_files = [
        "sdls/a.kickoff-special-teams.yml",
        "sdls/b.left-and-right-tackle.yml",
        "sdls/c.left-and-right-forwards.yml",
    ];

    // 2. Deploy snapshot & seed node
    client.deploy_sdl(1, sdl_files[0]).await?;

    // 3. Update SDL with node info
    client.update_sdl_with_node_info(1, sdl_files[1]).await?;

    // 4. Deploy L/R Tackles
    client.deploy_sdl(2, sdl_files[1]).await?;

    // 5. Update SDL with node info
    client.update_sdl_with_node_info(2, sdl_files[2]).await?;

    // 6. Deploy L/R Forwards
    client.deploy_sdl(3, sdl_files[2]).await?;

    // Print summary
    println!("[INFO] All deployments completed successfully!");
    println!("[INFO] Deployment Summary:");

    for (sdl_file, info) in &client.deployments {
        println!(
            "  {}: DSEQ={}, Provider={}",
            sdl_file, info.dseq, info.provider
        );
    }

    Ok(())
}

```