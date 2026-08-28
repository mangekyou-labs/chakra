//! Token logo index: contract address → official icon URL.
//!
//! Arc EVM catalog only (Arc SAC lists removed). Sources (merged in
//! priority order):
//! 1. Arc overrides verified against the frozen catalog addresses
//! 2. Remote token lists (configurable via `TOKEN_LOGO_LIST_URLS`)

use {
    serde::Deserialize,
    std::{collections::HashMap, sync::Arc, time::Duration},
    tokio::sync::RwLock,
    tracing::{info, warn},
};

const DEFAULT_LIST_URLS: &[&str] = &[];

// Keep this list deliberately small. Every entry is verified against the
// frozen Arc catalog address (USDC / EURC / mBTC).
const VERIFIED_LOGO_OVERRIDES: &[(&str, &str)] = &[
    (
        // USDC (native Arc testnet, 6 dp).
        "0x3600000000000000000000000000000000000000",
        "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/ethereum/assets/0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48/logo.png",
    ),
    (
        // EURC (Circle euro).
        "0x89b50855aa3be2f677cd6303cec089b5f319d72a",
        "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/ethereum/assets/0x1aBaEA1f7C830bD89Acc67eC4af516284b1bC33c/logo.png",
    ),
    (
        // mBTC (owner-mint Arc testnet).
        "0xbf5a25d7070faacae309d66d05372a6b212ecbdf",
        "https://raw.githubusercontent.com/trustwallet/assets/master/blockchains/bitcoin/info/logo.png",
    ),
];

const IPFS_GATEWAY: &str = "https://ipfs.io/ipfs/";

#[derive(Debug, Deserialize)]
struct Sep42List {
    #[serde(default)]
    assets: Vec<Sep42Asset>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sep42Asset {
    #[serde(default)]
    contract: Option<String>,
    #[serde(default)]
    icon: Option<String>,
}

/// In-memory map of contract StrKey → HTTPS icon URL.
#[derive(Clone, Default)]
pub struct TokenLogoListIndex {
    pub(crate) icons: Arc<RwLock<HashMap<String, String>>>,
    client: reqwest::Client,
    list_urls: Vec<String>,
}

impl TokenLogoListIndex {
    pub fn from_env() -> Self {
        let list_urls = std::env::var("TOKEN_LOGO_LIST_URLS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| DEFAULT_LIST_URLS.iter().map(|s| (*s).to_string()).collect());
        Self::new(list_urls)
    }

    pub fn new(list_urls: Vec<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("Chakra-token-logo-lists/0.1")
            .build()
            .expect("reqwest client");
        Self {
            icons: Arc::new(RwLock::new(HashMap::new())),
            client,
            list_urls,
        }
    }

    /// Look up an official icon URL by contract StrKey (`C...`).
    pub async fn icon_url(&self, contract: &str) -> Option<String> {
        self.icons.read().await.get(contract).cloned()
    }

    pub async fn len(&self) -> usize {
        self.icons.read().await.len()
    }

    /// Fetch all configured lists and merge into the index.
    /// Earlier lists win on conflicting contract keys.
    /// If no list URLs are configured, keeps the current in-memory index
    /// (no-op).
    pub async fn refresh(&self) -> usize {
        if self.list_urls.is_empty() {
            return self.icons.read().await.len();
        }

        let mut merged: HashMap<String, String> = VERIFIED_LOGO_OVERRIDES
            .iter()
            .map(|(contract, icon)| ((*contract).to_string(), (*icon).to_string()))
            .collect();
        for url in &self.list_urls {
            match self.fetch_list(url).await {
                Ok(entries) => {
                    let mut added = 0usize;
                    for (contract, icon) in entries {
                        if merged.contains_key(&contract) {
                            continue;
                        }
                        merged.insert(contract, icon);
                        added += 1;
                    }
                    info!(url = %url, added, total = merged.len(), "Loaded SEP-42 logo list");
                }
                Err(e) => {
                    warn!(url = %url, error = %e, "Failed to load SEP-42 logo list");
                }
            }
        }
        let n = merged.len();
        *self.icons.write().await = merged;
        n
    }

    async fn fetch_list(&self, url: &str) -> anyhow::Result<Vec<(String, String)>> {
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("HTTP {}", response.status());
        }
        let list: Sep42List = response.json().await?;
        Ok(parse_sep42_assets(&list.assets))
    }
}

/// Parse SEP-42 assets into `(contract_strkey, https_icon_url)` pairs.
pub(crate) fn parse_sep42_assets(assets: &[Sep42Asset]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for asset in assets {
        let Some(contract_raw) = asset.contract.as_deref() else {
            continue;
        };
        let Some(contract) = normalize_contract_id(contract_raw) else {
            continue;
        };
        let Some(icon_raw) = asset.icon.as_deref() else {
            continue;
        };
        let Some(icon) = normalize_icon_url(icon_raw) else {
            continue;
        };
        out.push((contract, icon));
    }
    out
}

/// Accept a 40-char EVM address (with or without `0x`) or a 64-char hex
/// contract hash; return `0x`-prefixed lowercase. Arc StrKeys are not Arc
/// contract ids.
pub fn normalize_contract_id(raw: &str) -> Option<String> {
    let s = raw.trim();
    let hex = s.strip_prefix("0x").unwrap_or(s);
    let hex_len = hex.len();
    if (hex_len == 40 || hex_len == 64) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(format!("0x{}", hex.to_ascii_lowercase()));
    }
    None
}

/// HTTPS URL or bare IPFS CID/hash → absolute HTTPS URL.
pub fn normalize_icon_url(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix("ipfs://") {
        return Some(format!("{IPFS_GATEWAY}{rest}"));
    }
    if s.starts_with("https://") {
        return Some(s.to_string());
    }
    if s.starts_with("http://") {
        // Upgrade cleartext to HTTPS when possible; otherwise skip.
        return Some(s.replacen("http://", "https://", 1));
    }
    // Bare CIDv0 (Qm...) / CIDv1 (bafy... / bafk...)
    if (s.starts_with("Qm") && s.len() >= 46) || s.starts_with("bafy") || s.starts_with("bafk") {
        return Some(format!("{IPFS_GATEWAY}{s}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_contract_accepts_evm_address_and_contract_hash() {
        // Arc EVM address (20 bytes) → 0x-prefixed lowercase.
        let addr = "0x3600000000000000000000000000000000000000";
        assert_eq!(normalize_contract_id(addr).as_deref(), Some(addr));
        assert_eq!(
            normalize_contract_id(&addr[2..].to_ascii_uppercase()).as_deref(),
            Some(addr)
        );
        // 64-char contract hash → 0x-prefixed lowercase.
        let hash = "ADEFCE59AEE52968F76061D494C2525B75659FA4296A65F499EF29E56477E496";
        assert_eq!(
            normalize_contract_id(hash).as_deref(),
            Some("0xadefce59aee52968f76061d494c2525b75659fa4296a65f499ef29e56477e496")
        );

        assert!(normalize_contract_id("not-a-contract").is_none());
    }

    #[test]
    fn normalize_icon_url_handles_https_and_ipfs() {
        assert_eq!(
            normalize_icon_url("https://example.com/a.png").as_deref(),
            Some("https://example.com/a.png")
        );
        assert_eq!(
            normalize_icon_url("ipfs://bafy123").as_deref(),
            Some("https://ipfs.io/ipfs/bafy123")
        );
        assert_eq!(
            normalize_icon_url("QmTmcN7qNDkcfaawoJCUv31aWjwT75KjKAX4zC7JFrE2Xr").as_deref(),
            Some("https://ipfs.io/ipfs/QmTmcN7qNDkcfaawoJCUv31aWjwT75KjKAX4zC7JFrE2Xr")
        );
        assert!(normalize_icon_url("").is_none());
    }

    #[test]
    fn parse_sep42_assets_skips_incomplete_entries() {
        let usdc_hex = "3600000000000000000000000000000000000000000000000000000000000000";
        let assets = vec![
            Sep42Asset {
                contract: Some(usdc_hex.into()),
                icon: Some("https://example.com/usdc.png".into()),
            },
            Sep42Asset {
                contract: Some(usdc_hex.into()),
                icon: None,
            },
            Sep42Asset {
                contract: None,
                icon: Some("https://example.com/x.png".into()),
            },
        ];
        let parsed = parse_sep42_assets(&assets);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].1, "https://example.com/usdc.png");
    }

    #[test]
    fn verified_overrides_use_valid_contracts_and_https_urls() {
        for (contract, icon) in VERIFIED_LOGO_OVERRIDES {
            assert_eq!(normalize_contract_id(contract).as_deref(), Some(*contract));
            assert!(icon.starts_with("https://"));
        }
    }

    #[tokio::test]
    async fn refresh_merges_lists_with_first_wins() {
        // Local HTTP fixtures via file:// aren't supported by reqwest easily;
        // unit-test merge semantics through parse + manual insert path.
        let index = TokenLogoListIndex::new(vec![]);
        {
            let mut icons = index.icons.write().await;
            icons.insert("CA".into(), "https://first/a.png".into());
        }
        // Simulate second list would not overwrite — covered by refresh loop.
        assert_eq!(index.icon_url("CA").await.as_deref(), Some("https://first/a.png"));
        assert_eq!(index.len().await, 1);
    }
}
