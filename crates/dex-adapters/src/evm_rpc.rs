//! Thin EVM JSON-RPC client for Arc (HTTP). No alloy/ethers — reqwest + hex.
//!
//! Only public Arc RPC endpoints are allowed by configuration:
//! `https://rpc.testnet.arc.io` plus the documented failovers
//! (Blockdaemon HTTP; dRPC HTTP/WS; QuickNode HTTP/WS). The Canteen `$RPC`
//! proxy (`rpc.testnet.arc-node.thecanteenapp.com`) is authenticated and
//! method-allowlisted — it does not expose `eth_subscribe` — so configuration
//! **fails** when a URL host resolves to it. Alchemy has no public Arc URL in
//! `connect-to-arc.md`; no invented Alchemy URLs may be configured either.

use {
    anyhow::{bail, Context, Result},
    serde_json::{json, Value},
    std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

// ─── RPC URL policy (T3.3.1) ────────────────────────────────────────────────

pub const ARC_RPC_HTTP: &str = "https://rpc.testnet.arc.io";
pub const ARC_RPC_WS: &str = "wss://rpc.testnet.arc.io";
pub const CANTEEN_RPC_HOST: &str = "rpc.testnet.arc-node.thecanteenapp.com";

/// Documented public HTTP failovers (design: Blockdaemon HTTP; dRPC; QuickNode).
pub const ALLOWED_HTTP_HOSTS: &[&str] = &[
    "rpc.testnet.arc.io",
    "rpc.blockdaemon.testnet.arc.io",
    "rpc.drpc.testnet.arc.io",
    "rpc.quicknode.testnet.arc.io",
];

/// Documented public WS endpoints (design: dRPC WS; QuickNode WS).
pub const ALLOWED_WS_HOSTS: &[&str] = &[
    "rpc.testnet.arc.io",
    "rpc.drpc.testnet.arc.io",
    "rpc.quicknode.testnet.arc.io",
];

fn host_of(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    Some(parsed.host_str()?.to_ascii_lowercase())
}

/// True when the URL points at the Canteen `$RPC` proxy.
pub fn is_canteen_rpc(url: &str) -> bool {
    host_of(url).as_deref() == Some(CANTEEN_RPC_HOST)
}

/// Allow an HTTP(S) URL only when its host is a documented public Arc endpoint.
pub fn evm_http_url_allowed(url: &str) -> bool {
    let parsed = match url::Url::parse(url) {
        Ok(p) => p,
        Err(_) => return false,
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    let host = parsed.host_str().map(|h| h.to_ascii_lowercase());
    match host.as_deref() {
        Some(host) => ALLOWED_HTTP_HOSTS.contains(&host) && !is_canteen_rpc(url),
        None => false,
    }
}

/// Allow a WS(S) URL only when its host is a documented public Arc WS endpoint.
pub fn evm_ws_url_allowed(url: &str) -> bool {
    let parsed = match url::Url::parse(url) {
        Ok(p) => p,
        Err(_) => return false,
    };
    if !matches!(parsed.scheme(), "ws" | "wss") {
        return false;
    }
    let host = parsed.host_str().map(|h| h.to_ascii_lowercase());
    match host.as_deref() {
        Some(host) => ALLOWED_WS_HOSTS.contains(&host) && !is_canteen_rpc(url),
        None => false,
    }
}

/// Validate a failover list; returns the filtered list or a descriptive error.
pub fn validate_http_urls(urls: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(urls.len());
    for url in urls {
        if !evm_http_url_allowed(url) {
            bail!("disallowed HTTP RPC URL (public Arc + documented failovers only, never Canteen $RPC): {url}");
        }
        out.push(url.clone());
    }
    Ok(out)
}

/// Validate a WS failover list; returns the filtered list or a descriptive error.
pub fn validate_ws_urls(urls: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(urls.len());
    for url in urls {
        if !evm_ws_url_allowed(url) {
            bail!("disallowed WS RPC URL (public Arc + documented failovers only, never Canteen $RPC): {url}");
        }
        out.push(url.clone());
    }
    Ok(out)
}

// ─── JSON-RPC request plumbing ──────────────────────────────────────────────

const REQUEST_TIMEOUT_SECS: u64 = 10;

/// One `eth_getLogs` log entry (JSON-RPC block-filter shape).
#[derive(Debug, Clone)]
pub struct EvmLog {
    /// Emitting contract (pool or factory), `0x`-prefixed.
    pub address: String,
    /// `0x`-prefixed 32-byte topics; `topics[0]` is the event signature hash.
    pub topics: Vec<String>,
    /// Hex-encoded non-indexed event data.
    pub data: String,
    pub block_number: Option<u64>,
    pub tx_hash: Option<String>,
    pub log_index: Option<u64>,
}

/// `eth_getLogs` filter object.
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    /// Addresses to match (OR). Empty = every address.
    pub addresses: Vec<String>,
    /// Topic0..N constraints. Each position may contain an OR-list; `None`
    /// is a wildcard. The watcher uses one topic-zero OR-list, which keeps
    /// the filter within the EVM four-topic limit.
    pub topics: Vec<Option<Vec<String>>>,
}

/// One failed JSON-RPC call. Carries the URL and, for JSON-RPC application
/// errors, the numeric error code.
#[derive(Debug)]
pub struct RpcError {
    pub url: String,
    /// JSON-RPC application error code; `None` for transport/HTTP failures.
    pub code: Option<i64>,
    /// Human-readable failure detail (server message / transport error).
    pub detail: String,
}

impl RpcError {
    /// Transport failures, HTTP non-2xx responses, and JSON-RPC `-32005`
    /// (rate limit / request limit exceeded) are transient: the client retries
    /// them with bounded exponential backoff and ordered URL failover.
    /// Deterministic application errors such as the `-32012` malformed-filter
    /// rejection are never retried — retrying them only wastes RPC budget and
    /// hides the config bug from the caller.
    pub fn retryable(&self) -> bool {
        match self.code {
            Some(-32_005) => true,
            Some(_) => false,
            None => true,
        }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(code) => write!(f, "RPC error from {}: code {code}: {}", self.url, self.detail),
            None => write!(f, "RPC POST failed: {}: {}", self.url, self.detail),
        }
    }
}

impl std::error::Error for RpcError {}

/// Minimal JSON-RPC client with ordered URL failover.
#[derive(Clone)]
pub struct EvmRpcClient {
    http: reqwest::Client,
    urls: Vec<String>,
    next: Arc<AtomicUsize>,
}

impl EvmRpcClient {
    pub fn new(urls: Vec<String>) -> Result<Self> {
        if urls.is_empty() {
            bail!("EvmRpcClient requires at least one URL");
        }
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .build()?,
            urls,
            next: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn single(url: &str) -> Result<Self> {
        Self::new(vec![url.to_string()])
    }

    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let mut last_error: Option<RpcError> = None;
        let mut delay_ms = 500u64;
        for _attempt in 0..8 {
            for offset in 0..self.urls.len() {
                let index = (self.next.load(Ordering::Relaxed) + offset) % self.urls.len();
                let url = &self.urls[index];
                let body = json!({
                    "jsonrpc": "2.0",
                    "id": 1u64,
                    "method": method,
                    "params": params,
                });
                match self.post_once(url, &body).await {
                    Ok(value) => {
                        self.next.store((index + 1) % self.urls.len(), Ordering::Relaxed);
                        return Ok(value);
                    }
                    Err(error) => {
                        // Only transient failures are retried: transport
                        // errors, HTTP 429/5xx, and JSON-RPC -32005 rate
                        // limits. Malformed-filter errors such as -32012 are
                        // deterministic and surface immediately.
                        if !error.retryable() {
                            return Err(anyhow::Error::new(error));
                        }
                        last_error = Some(error);
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            delay_ms = (delay_ms.saturating_mul(2)).min(30_000);
        }
        Err(match last_error {
            Some(error) => anyhow::Error::new(error),
            None => anyhow::anyhow!("no RPC URL to try"),
        })
    }

    async fn post_once(&self, url: &str, body: &Value) -> std::result::Result<Value, RpcError> {
        let response = self.http.post(url).json(body).send().await.map_err(|error| RpcError {
            url: url.to_string(),
            code: None,
            detail: format!("{error}"),
        })?;
        if !response.status().is_success() {
            return Err(RpcError {
                url: url.to_string(),
                code: None,
                detail: format!("HTTP {}", response.status()),
            });
        }
        let value: Value = response.json().await.map_err(|error| RpcError {
            url: url.to_string(),
            code: None,
            detail: format!("response parse failed: {error}"),
        })?;
        if let Some(error) = value.get("error") {
            return Err(RpcError {
                url: url.to_string(),
                code: error.get("code").and_then(Value::as_i64),
                detail: error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| error.to_string()),
            });
        }
        value.get("result").cloned().ok_or_else(|| RpcError {
            url: url.to_string(),
            code: None,
            detail: "response missing result".to_string(),
        })
    }

    pub async fn eth_block_number(&self) -> Result<u64> {
        let result = self.request("eth_blockNumber", json!([])).await?;
        let hex = result.as_str().context("eth_blockNumber result not a hex string")?;
        parse_hex_u64(hex)
    }

    /// `eth_call` returning the raw `0x`-prefixed 32-byte result word.
    pub async fn eth_call(&self, to: &str, data: &str) -> Result<String> {
        let params = json!([{ "to": to, "data": data }, "latest"]);
        let result = self.request("eth_call", params).await?;
        result
            .as_str()
            .map(str::to_string)
            .context("eth_call result not a hex string")
    }

    /// `eth_getBalance` returning the raw `0x`-prefixed balance hex.
    pub async fn eth_get_balance(&self, account: &str) -> Result<String> {
        let params = json!([account, "latest"]);
        let result = self.request("eth_getBalance", params).await?;
        result
            .as_str()
            .map(str::to_string)
            .context("eth_getBalance result not a hex string")
    }

    /// `eth_getCode` returning the raw `0x`-prefixed bytecode hex (empty
    /// `0x`/`0x0` when the address has no code). Used by manifest venue
    /// verification (2026-08-29).
    pub async fn eth_get_code(&self, address: &str) -> Result<String> {
        let params = json!([address, "latest"]);
        let result = self.request("eth_getCode", params).await?;
        result
            .as_str()
            .map(str::to_string)
            .context("eth_getCode result not a hex string")
    }

    /// Return the block timestamp for an already-confirmed block.
    pub async fn eth_get_block_timestamp(&self, block: u64) -> Result<u64> {
        let result = self
            .request("eth_getBlockByNumber", json!([format!("0x{block:x}"), false]))
            .await?;
        let timestamp = result
            .get("timestamp")
            .and_then(Value::as_str)
            .context("eth_getBlockByNumber result missing timestamp")?;
        parse_hex_u64(timestamp)
    }

    /// Return transaction input calldata, if the transaction is still
    /// available from the node.
    pub async fn eth_get_transaction_input(&self, tx_hash: &str) -> Result<Option<String>> {
        let result = self.request("eth_getTransactionByHash", json!([tx_hash])).await?;
        Ok(result.get("input").and_then(Value::as_str).map(str::to_string))
    }

    pub async fn eth_get_logs(&self, filter: &LogFilter) -> Result<Vec<EvmLog>> {
        let mut object = serde_json::Map::new();
        if let Some(from) = filter.from_block {
            object.insert("fromBlock".into(), json!(format!("0x{from:x}")));
        }
        if let Some(to) = filter.to_block {
            object.insert("toBlock".into(), json!(format!("0x{to:x}")));
        }
        if !filter.addresses.is_empty() {
            object.insert(
                "address".into(),
                json!(filter.addresses.iter().map(|a| a.as_str()).collect::<Vec<_>>()),
            );
        }
        if !filter.topics.is_empty() {
            object.insert(
                "topics".into(),
                json!(filter
                    .topics
                    .iter()
                    .map(|t| t.as_ref().map(|values| {
                        if values.len() == 1 {
                            serde_json::Value::String(values[0].clone())
                        } else {
                            serde_json::Value::Array(values.iter().cloned().map(serde_json::Value::String).collect())
                        }
                    }))
                    .collect::<Vec<_>>()),
            );
        }
        let params = json!([object]);
        let result = self.request("eth_getLogs", params).await?;
        let logs = result.as_array().context("eth_getLogs result not an array")?;
        logs.iter().map(log_from_json).collect::<Result<Vec<_>>>()
    }
}

impl EvmLog {
    /// Parse one JSON-RPC log object (also used for `eth_subscription`
    /// notification `params.result` from the WS path).
    pub fn from_json(value: &Value) -> Result<Self> {
        log_from_json(value)
    }
}

fn log_from_json(value: &Value) -> Result<EvmLog> {
    let topics = value
        .get("topics")
        .and_then(Value::as_array)
        .context("log missing topics")?
        .iter()
        .filter_map(|t| t.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    Ok(EvmLog {
        address: value
            .get("address")
            .and_then(Value::as_str)
            .context("log missing address")?
            .to_string(),
        topics,
        data: value.get("data").and_then(Value::as_str).unwrap_or("0x").to_string(),
        block_number: value
            .get("blockNumber")
            .and_then(Value::as_str)
            .and_then(|hex| parse_hex_u64(hex).ok()),
        tx_hash: value.get("transactionHash").and_then(Value::as_str).map(str::to_string),
        log_index: value
            .get("logIndex")
            .and_then(Value::as_str)
            .and_then(|hex| parse_hex_u64(hex).ok()),
    })
}

// ─── Hex helpers ────────────────────────────────────────────────────────────

/// Decode `0x`-prefixed hex into bytes (must be even-length).
pub fn decode_hex(hex: &str) -> Result<Vec<u8>> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    if !hex.len().is_multiple_of(2) {
        bail!("odd-length hex: {hex}");
    }
    hex::decode(hex).map_err(|e| anyhow::anyhow!("invalid hex: {e}"))
}

pub fn parse_hex_u64(hex: &str) -> Result<u64> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    u64::from_str_radix(hex, 16).map_err(|e| anyhow::anyhow!("invalid hex u64: {e}"))
}

/// Decode a 32-byte word (as `0x`-prefixed hex) into a `u128`.
pub fn word_to_u128(hex: &str) -> Result<u128> {
    let bytes = decode_hex(hex)?;
    if bytes.len() > 32 {
        bail!("word longer than 32 bytes");
    }
    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(u128::from_be_bytes(padded[16..].try_into().unwrap()))
}

/// Decode a 32-byte word into an `i32` (two's-complement 256-bit, values that
/// fit in `i32` — tick/int24 etc.).
pub fn word_to_i32(hex: &str) -> Result<i32> {
    let bytes = decode_hex(hex)?;
    if bytes.len() > 32 {
        bail!("word longer than 32 bytes");
    }
    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(&bytes);
    let low = u32::from_be_bytes(padded[28..].try_into().unwrap());
    if padded[0] & 0x80 != 0 {
        // Negative 256-bit value (sign bit set): two's complement toward -2^32.
        Ok((low as i64 - (1i64 << 32)) as i32)
    } else {
        Ok(low as i32)
    }
}

/// Decode a 32-byte word into little-endian `[u64; 4]` limbs (U256 as stored in
/// `ClmmPoolSnapshot.sqrt_price_x96`).
///
/// The word is a big-endian 256-bit integer; limb `i` covers bits
/// `[64i, 64i+64)` and therefore comes from the *rightmost* byte window
/// `[24-8i, 32-8i)`, read big-endian.
pub fn word_to_u256_limbs(hex: &str) -> Result<[u64; 4]> {
    let bytes = decode_hex(hex)?;
    if bytes.len() > 32 {
        bail!("word longer than 32 bytes");
    }
    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(&bytes);
    let mut limbs = [0u64; 4];
    for (i, limb) in limbs.iter_mut().enumerate() {
        let start = 24 - 8 * i;
        let mut window = [0u8; 8];
        window.copy_from_slice(&padded[start..start + 8]);
        *limb = u64::from_be_bytes(window);
    }
    Ok(limbs)
}

/// Fixture JSON-RPC HTTP server (test-only; never touches live Arc).
/// Public so api-server integration tests can spin up a fixture RPC.
#[cfg(any(test, feature = "test-fixture"))]
pub mod fixture {
    use {
        super::*,
        std::sync::Arc,
        tokio::io::{AsyncReadExt, AsyncWriteExt},
        tokio::net::{TcpListener, TcpStream},
    };

    /// Spawn a fixture JSON-RPC server. Handler: `Fn(method, params) -> Result<Value, Value>`
    /// where `Err` becomes a JSON-RPC error response.
    ///
    /// The server runs on a dedicated std thread owning its own tokio runtime,
    /// so it keeps accepting connections for the whole test process and never
    /// touches the calling test's runtime (test-only).
    pub fn spawn(
        handler: impl Fn(&str, &Value) -> Result<Value, Value> + Send + Sync + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        let handler = Arc::new(handler);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let thread_handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("fixture runtime");
            runtime.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let _ = tx.send(format!("http://{addr}"));
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        continue;
                    };
                    let _ = handle_request(&mut socket, handler.as_ref()).await;
                }
            });
        });
        let url = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("fixture server url timeout");
        (url, thread_handle)
    }

    /// Spawn a fixture JSON-RPC server whose handler returns the *complete*
    /// response object (`{"result": …}` or `{"error": {"code": -32005, …}}`).
    /// The request id is merged in so handlers never need to echo it. Used for
    /// error-code-specific tests (rate limits vs malformed filters).
    pub fn spawn_raw(
        handler: impl Fn(&str, &Value) -> Value + Send + Sync + 'static,
    ) -> (String, std::thread::JoinHandle<()>) {
        let handler = Arc::new(handler);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let thread_handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("fixture runtime");
            runtime.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let _ = tx.send(format!("http://{addr}"));
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        continue;
                    };
                    let _ = handle_raw_request(&mut socket, handler.as_ref()).await;
                }
            });
        });
        let url = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("fixture server url timeout");
        (url, thread_handle)
    }

    const MAX_HEAD: usize = 4096;

    async fn handle_raw_request(
        socket: &mut TcpStream,
        handler: &(dyn Fn(&str, &Value) -> Value + Send + Sync),
    ) -> std::io::Result<()> {
        let mut buf = [0u8; MAX_HEAD];
        let mut filled = 0usize;
        let (content_length, body_start) = loop {
            let n = socket.read(&mut buf[filled..]).await?;
            if n == 0 {
                return Ok(());
            }
            filled += n;
            let hay = std::str::from_utf8(&buf[..filled]).unwrap_or("");
            if let Some(pos) = hay.find("\r\n\r\n") {
                let head = &hay[..pos];
                let content_length = head
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        (k.eq_ignore_ascii_case("content-length"))
                            .then(|| v.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                break (content_length, pos + 4);
            }
            if filled >= MAX_HEAD {
                return Ok(());
            }
        };
        if filled < body_start + content_length {
            let mut rest = vec![0u8; body_start + content_length - filled];
            socket.read_exact(&mut rest).await?;
            buf[filled..filled + rest.len()].copy_from_slice(&rest);
        }
        let body = &buf[body_start..body_start + content_length];
        let request: Value = serde_json::from_slice(body).unwrap_or(json!({"id": 0}));
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let mut response = handler(method, &params);
        if let Some(object) = response.as_object_mut() {
            object.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
            object.insert("id".to_string(), id);
        }
        let body = serde_json::to_vec(&response).unwrap();
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await?;
        socket.write_all(&body).await?;
        socket.flush().await?;
        Ok(())
    }

    async fn handle_request(
        socket: &mut TcpStream,
        handler: &(dyn Fn(&str, &Value) -> Result<Value, Value> + Send + Sync),
    ) -> std::io::Result<()> {
        let mut buf = [0u8; MAX_HEAD];
        let mut filled = 0usize;
        let (content_length, body_start) = loop {
            let n = socket.read(&mut buf[filled..]).await?;
            if n == 0 {
                return Ok(());
            }
            filled += n;
            let hay = std::str::from_utf8(&buf[..filled]).unwrap_or("");
            if let Some(pos) = hay.find("\r\n\r\n") {
                let head = &hay[..pos];
                let content_length = head
                    .lines()
                    .find_map(|line| {
                        let (k, v) = line.split_once(':')?;
                        (k.eq_ignore_ascii_case("content-length"))
                            .then(|| v.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                break (content_length, pos + 4);
            }
            if filled >= MAX_HEAD {
                return Ok(());
            }
        };
        if filled < body_start + content_length {
            let mut rest = vec![0u8; body_start + content_length - filled];
            socket.read_exact(&mut rest).await?;
            buf[filled..filled + rest.len()].copy_from_slice(&rest);
        }
        let body = &buf[body_start..body_start + content_length];
        let request: Value = serde_json::from_slice(body).unwrap_or(json!({"id": 0}));
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let response = match handler(method, &params) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32000, "message": format!("{error}")}
            }),
        };
        let body = serde_json::to_vec(&response).unwrap();
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await?;
        socket.write_all(&body).await?;
        socket.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::evm_rpc::fixture, tokio::net::TcpListener};

    // ─── RPC URL policy tests ─────────────────────────────────

    #[test]
    fn rejects_canteen_proxy_rpc() {
        assert!(is_canteen_rpc("https://rpc.testnet.arc-node.thecanteenapp.com"));
        assert!(is_canteen_rpc("https://rpc.testnet.arc-node.thecanteenapp.com/v2/xyz"));
        assert!(!evm_http_url_allowed("https://rpc.testnet.arc-node.thecanteenapp.com"));
        assert!(!evm_ws_url_allowed("wss://rpc.testnet.arc-node.thecanteenapp.com"));
    }

    #[test]
    fn allows_public_arc_and_documented_failovers() {
        for host in ALLOWED_HTTP_HOSTS {
            assert!(
                evm_http_url_allowed(&format!("https://{host}")),
                "http url should be allowed: {host}"
            );
        }
        for host in ALLOWED_WS_HOSTS {
            assert!(
                evm_ws_url_allowed(&format!("wss://{host}")),
                "ws url should be allowed: {host}"
            );
        }
        // HTTP list may include Blockdaemon; WS list may not.
        assert!(evm_http_url_allowed("https://rpc.blockdaemon.testnet.arc.io"));
        assert!(!evm_ws_url_allowed("wss://rpc.blockdaemon.testnet.arc.io"));
        // Scheme/scheme-host mismatch rejected.
        assert!(!evm_http_url_allowed("wss://rpc.testnet.arc.io"));
        assert!(!evm_ws_url_allowed("https://rpc.testnet.arc.io"));
    }

    #[test]
    fn rejects_invented_alchemy_url() {
        assert!(!evm_http_url_allowed("https://arc-mainnet.g.alchemy.com/v2/xxxx"));
        assert!(!evm_http_url_allowed("https://arc-testnet.alchemy.com/v2/xxxx"));
        assert!(!evm_ws_url_allowed("wss://arc-mainnet.g.alchemy.com/v2/xxxx"));
    }

    #[test]
    fn validate_lists_reject_canteen_and_accept_documented() {
        let ok =
            validate_http_urls(&[ARC_RPC_HTTP.to_string(), "https://rpc.drpc.testnet.arc.io".to_string()]).unwrap();
        assert_eq!(ok.len(), 2);
        assert!(validate_http_urls(&[CANTEEN_RPC_HOST.to_string()]).is_err());
        assert!(validate_http_urls(&["not a url".to_string()]).is_err());
        let ws = validate_ws_urls(&[ARC_RPC_WS.to_string(), "wss://rpc.quicknode.testnet.arc.io".to_string()]).unwrap();
        assert_eq!(ws.len(), 2);
        assert!(validate_ws_urls(&["https://rpc.testnet.arc.io".to_string()]).is_err());
    }

    // ─── Client tests (fixture server, never live Arc) ────────

    #[tokio::test]
    async fn eth_block_number_parses_fixture_response() {
        let (url, _server) = fixture::spawn(|method, _| {
            assert_eq!(method, "eth_blockNumber");
            Ok(json!("0x10"))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        assert_eq!(client.eth_block_number().await.unwrap(), 16);
    }

    #[tokio::test]
    async fn eth_call_sends_to_and_data_and_returns_raw_word() {
        let (url, _server) = fixture::spawn(|method, params| {
            assert_eq!(method, "eth_call");
            let call = &params[0];
            assert_eq!(call["to"], "0xAAAA");
            assert_eq!(call["data"], "0xdeadbeef");
            assert_eq!(params[1], "latest");
            Ok(json!(
                "0x000000000000000000000000000000000000000000000000000000000000002a"
            ))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let word = client.eth_call("0xAAAA", "0xdeadbeef").await.unwrap();
        assert_eq!(word_to_u128(&word).unwrap(), 42);
    }

    #[tokio::test]
    async fn eth_get_logs_parses_log_objects() {
        let (url, _server) = fixture::spawn(|method, params| {
            assert_eq!(method, "eth_getLogs");
            let filter = &params[0];
            assert_eq!(filter["fromBlock"], "0xa");
            assert_eq!(filter["toBlock"], "0xb");
            assert_eq!(filter["address"][0], "0xpool1");
            Ok(json!([
                {
                    "address": "0xpool1",
                    "topics": ["0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822"],
                    "data": "0x0000000000000000000000000000000000000000000000000000000000000001",
                    "blockNumber": "0xa",
                    "transactionHash": "0xabc",
                    "logIndex": "0x0"
                }
            ]))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let filter = LogFilter {
            from_block: Some(10),
            to_block: Some(11),
            addresses: vec!["0xpool1".to_string()],
            topics: vec![],
        };
        let logs = client.eth_get_logs(&filter).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].address, "0xpool1");
        assert_eq!(logs[0].block_number, Some(10));
        assert_eq!(logs[0].log_index, Some(0));
        assert_eq!(logs[0].tx_hash.as_deref(), Some("0xabc"));
        assert_eq!(logs[0].topics.len(), 1);
    }

    #[tokio::test]
    async fn serializes_topic_or_list_at_single_position() {
        let (url, _server) = fixture::spawn(|method, params| {
            assert_eq!(method, "eth_getLogs");
            let topics = params[0]["topics"].as_array().unwrap();
            assert_eq!(topics.len(), 1);
            assert!(topics[0].is_array());
            assert_eq!(topics[0].as_array().unwrap().len(), 2);
            Ok(json!([]))
        });
        let client = EvmRpcClient::single(&url).unwrap();
        client
            .eth_get_logs(&LogFilter {
                topics: vec![Some(vec!["0xa".into(), "0xb".into()])],
                ..Default::default()
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn fails_over_to_next_url_on_connection_error() {
        // First URL points at a closed port; second is the fixture.
        let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_addr = closed.local_addr().unwrap();
        drop(closed); // port now refuses connections
        let (url, _server) = fixture::spawn(|method, _| {
            assert_eq!(method, "eth_blockNumber");
            Ok(json!("0x2a"))
        });
        let client = EvmRpcClient::new(vec![format!("http://{closed_addr}"), url]).unwrap();
        assert_eq!(client.eth_block_number().await.unwrap(), 42);
        assert_eq!(client.urls().len(), 2);
    }

    #[tokio::test]
    async fn surfaces_jsonrpc_errors_as_failures() {
        let (url, _server) = fixture::spawn(|_method, _| Err(json!("boom")));
        let client = EvmRpcClient::single(&url).unwrap();
        let error = client.eth_block_number().await.unwrap_err();
        assert!(format!("{error}").contains("boom"));
    }

    #[test]
    fn rpc_error_classification_retries_rate_limit_but_not_malformed_filters() {
        let rate = RpcError {
            url: "http://u".into(),
            code: Some(-32_005),
            detail: "rate limit".into(),
        };
        assert!(rate.retryable(), "-32005 rate limits are retried");
        let malformed = RpcError {
            url: "http://u".into(),
            code: Some(-32_012),
            detail: "invalid filter".into(),
        };
        assert!(!malformed.retryable(), "-32012 malformed filters are never retried");
        let method = RpcError {
            url: "http://u".into(),
            code: Some(-32_601),
            detail: "method not found".into(),
        };
        assert!(!method.retryable(), "unknown application codes surface immediately");
        let transport = RpcError {
            url: "http://u".into(),
            code: None,
            detail: "connection refused".into(),
        };
        assert!(transport.retryable(), "transport failures are retried");
    }

    #[tokio::test]
    async fn rate_limit_error_is_retried_with_backoff_then_succeeds() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_inside = calls.clone();
        let (url, _server) = fixture::spawn_raw(move |method, _| {
            assert_eq!(method, "eth_blockNumber");
            let n = calls_inside.fetch_add(1, Ordering::Relaxed);
            if n == 0 {
                json!({"error": {"code": -32005, "message": "rate limit exceeded"}})
            } else {
                json!({"result": "0x2a"})
            }
        });
        let client = EvmRpcClient::single(&url).unwrap();
        assert_eq!(client.eth_block_number().await.unwrap(), 42);
        assert_eq!(
            calls.load(Ordering::Relaxed),
            2,
            "exactly one bounded retry after -32005"
        );
    }

    #[tokio::test]
    async fn malformed_filter_error_is_never_retried() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls_inside = calls.clone();
        let (url, _server) = fixture::spawn_raw(move |method, _| {
            assert_eq!(method, "eth_getLogs");
            calls_inside.fetch_add(1, Ordering::Relaxed);
            json!({"error": {"code": -32012, "message": "filter exceeds allowed topics"}})
        });
        let client = EvmRpcClient::single(&url).unwrap();
        let error = client.eth_get_logs(&LogFilter::default()).await.unwrap_err();
        assert!(format!("{error}").contains("filter exceeds allowed topics"));
        assert_eq!(calls.load(Ordering::Relaxed), 1, "-32012 must surface without retries");
    }

    #[tokio::test]
    async fn decode_helpers_handle_words_and_limbs() {
        assert_eq!(word_to_u128("0x2a").unwrap(), 42);
        assert_eq!(
            word_to_u128("0x000000000000000000000000000000000000000000000000000000000000002a").unwrap(),
            42
        );
        // Negative int24 tick: 0xFFFF...FED4 (tick = -300).
        assert_eq!(
            word_to_i32("0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffed4").unwrap(),
            -300
        );
        assert_eq!(word_to_i32("0xb4").unwrap(), 180);
        // sqrtPriceX96 = 10 * 2^96 → limb1 = 10 * 2^32 = 0xA00000000.
        assert_eq!(
            word_to_u256_limbs(&format!("0x{:0>64x}", 10u128 << 96)).unwrap(),
            [0, 0xa00000000, 0, 0]
        );
        // 2^64 → limb 1.
        assert_eq!(
            word_to_u256_limbs(&format!("0x{:0>64x}", 1u128 << 64)).unwrap(),
            [0, 1, 0, 0]
        );
        assert_eq!(parse_hex_u64("0x10").unwrap(), 16);
    }
}
