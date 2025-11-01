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
pub async fn get_task_ip() -> Result<String> {
    let client = reqwest::Client::new();

    // Fetch task metadata
    let task: EcsTask = client
        .get(ECS_TASK_URI)
        .send()
        .await
        .context("Failed to fetch ECS task metadata")?
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

/// Get the task ARN from ECS metadata service
pub async fn get_task_arn() -> Result<String> {
    let client = reqwest::Client::new();

    let metadata: EcsMetadata = client
        .get(ECS_METADATA_URI)
        .send()
        .await
        .context("Failed to fetch ECS metadata")?
        .json()
        .await
        .context("Failed to parse ECS metadata")?;

    println!("Detected task ARN: {}", metadata.task_arn);
    Ok(metadata.task_arn)
}
