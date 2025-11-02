use anyhow::{Context, Result};
use serde::Deserialize;

const ECS_METADATA_URI: &str = "http://169.254.170.2/v2/metadata";
const ECS_TASK_URI: &str = "http://169.254.170.2/v2/task";

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EcsMetadata {
    #[serde(rename = "Cluster")]
    cluster: String,
    #[serde(rename = "TaskARN")]
    task_arn: String,
    #[serde(rename = "Family")]
    family: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EcsTask {
    #[serde(rename = "Cluster")]
    cluster: String,
    #[serde(rename = "TaskARN")]
    task_arn: String,
    #[serde(rename = "Family")]
    family: String,
    #[serde(rename = "Revision")]
    revision: String,
    #[serde(rename = "DesiredStatus")]
    desired_status: String,
    #[serde(rename = "KnownStatus")]
    known_status: String,
    #[serde(rename = "Containers")]
    containers: Vec<EcsContainer>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EcsContainer {
    #[serde(rename = "DockerId")]
    docker_id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "DockerName")]
    docker_name: String,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "Networks")]
    networks: Vec<EcsNetwork>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct EcsNetwork {
    #[serde(rename = "NetworkMode")]
    network_mode: String,
    #[serde(rename = "IPv4Addresses")]
    ipv4_addresses: Vec<String>,
}

/// Get the task's private IP address from ECS metadata service
/// Falls back to 127.0.0.1 for local development
pub async fn get_task_ip() -> Result<String> {
    // Check for environment variable override first
    if let Ok(ip) = std::env::var("NODE_IP") {
        println!("Using NODE_IP from environment: {}", ip);
        return Ok(ip);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;

    // Try to fetch task metadata
    let task_result = client
        .get(ECS_TASK_URI)
        .send()
        .await;

    match task_result {
        Ok(response) => {
            let task: EcsTask = response
                .json()
                .await
                .context("Failed to parse ECS task metadata")?;

            // Extract IP from first container's first network
            let ip = task
                .containers
                .first()
                .context("No containers found in task metadata")?
                .networks
                .first()
                .context("No networks found in container metadata")?
                .ipv4_addresses
                .first()
                .context("No IPv4 addresses found in network metadata")?
                .clone();

            println!("Detected task IP from ECS metadata: {}", ip);
            Ok(ip)
        }
        Err(_) => {
            // Fallback for local development
            let fallback_ip = "127.0.0.1".to_string();
            println!("⚠️  ECS metadata not available (local development mode)");
            println!("   Using fallback IP: {}", fallback_ip);
            println!("   Set NODE_IP environment variable to override");
            Ok(fallback_ip)
        }
    }
}

/// Get the task ARN from ECS metadata service
/// Falls back to a local identifier for local development
pub async fn get_task_arn() -> Result<String> {
    // Check for environment variable override first
    if let Ok(arn) = std::env::var("TASK_ARN") {
        println!("Using TASK_ARN from environment: {}", arn);
        return Ok(arn);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;

    let metadata_result = client
        .get(ECS_METADATA_URI)
        .send()
        .await;

    match metadata_result {
        Ok(response) => {
            let metadata: EcsMetadata = response
                .json()
                .await
                .context("Failed to parse ECS metadata")?;

            println!("Detected task ARN: {}", metadata.task_arn);
            Ok(metadata.task_arn)
        }
        Err(_) => {
            // Fallback for local development
            let fallback_arn = format!("local-task-{}", std::process::id());
            println!("⚠️  ECS metadata not available (local development mode)");
            println!("   Using fallback ARN: {}", fallback_arn);
            println!("   Set TASK_ARN environment variable to override");
            Ok(fallback_arn)
        }
    }
}
