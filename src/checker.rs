use std::time::Duration;

use anyhow::Result;
use reqwest::Client;

use crate::parser::Proxy;

/// The result of checking a single proxy.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub proxy: Proxy,
    pub alive: bool,
    pub latency_ms: Option<u64>,
}

/// URL used to verify that the proxy works.
const CHECK_URL: &str = "http://www.gstatic.com/generate_204";

/// Build a reqwest `Client` configured to route all traffic through the given proxy.
fn build_client(proxy: &Proxy, timeout: Duration) -> Result<Client> {
    let proxy_url = proxy.to_url();
    let rp = reqwest::Proxy::all(&proxy_url)?;
    let client = Client::builder()
        .proxy(rp)
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .build()?;
    Ok(client)
}

/// Check whether a proxy is alive. Returns a `CheckResult` in all cases
/// (errors are mapped to `alive = false`).
pub async fn check_proxy(proxy: Proxy, timeout: Duration) -> CheckResult {
    match do_check(&proxy, timeout).await {
        Ok(latency_ms) => CheckResult {
            proxy,
            alive: true,
            latency_ms: Some(latency_ms),
        },
        Err(_) => CheckResult {
            proxy,
            alive: false,
            latency_ms: None,
        },
    }
}

async fn do_check(proxy: &Proxy, timeout: Duration) -> Result<u64> {
    let client = build_client(proxy, timeout)?;
    let start = std::time::Instant::now();
    let resp = client.get(CHECK_URL).send().await?;
    let latency = start.elapsed().as_millis() as u64;
    // Google's generate_204 returns HTTP 204 on success.
    // We accept any 2xx response as a sign that the proxy works.
    if resp.status().is_success() {
        Ok(latency)
    } else {
        anyhow::bail!("Non-success status: {}", resp.status());
    }
}
