//! Token metadata cache: persists token symbol/name to a JSON file.
//! On startup, loads from file. In background, resolves unknown tokens via RPC.

use {
    crate::{token_logo::TokenLogoCache, token_logo_lists::TokenLogoListIndex},
    serde::{Deserialize, Serialize},
    std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::Arc,
    },
    tokio::sync::RwLock,
    tracing::{info, warn},
};

const METADATA_FILE: &str = "data/token_metadata.json";

fn load_metadata_file(path: &Path) -> HashMap<String, TokenMetadata> {
    match std::fs::read_to_string(path) {
        Ok(data) => match serde_json::from_str::<MetadataCache>(&data) {
            Ok(file_cache) => {
                info!("Loaded {} token metadata entries from cache", file_cache.tokens.len());
                file_cache.tokens
            }
            Err(_) => HashMap::new(),
        },
        Err(_) => HashMap::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogoKind {
    Official,
    #[default]
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub contract: String,
    pub symbol: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_kind: Option<LogoKind>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataCache {
    tokens: HashMap<String, TokenMetadata>,
}

pub struct TokenMetadataStore {
    cache: Arc<RwLock<HashMap<String, TokenMetadata>>>,
    logo_cache: Arc<TokenLogoCache>,
    logo_lists: Arc<TokenLogoListIndex>,
    metadata_file: PathBuf,
}

impl TokenMetadataStore {
    pub fn new() -> Self {
        Self::with_logo_cache(TokenLogoCache::from_env(), TokenLogoListIndex::from_env())
    }

    pub fn with_logo_cache(logo_cache: TokenLogoCache, logo_lists: TokenLogoListIndex) -> Self {
        let metadata_file = PathBuf::from(METADATA_FILE);
        let cache = load_metadata_file(&metadata_file);
        Self {
            cache: Arc::new(RwLock::new(cache)),
            logo_cache: Arc::new(logo_cache),
            logo_lists: Arc::new(logo_lists),
            metadata_file,
        }
    }

    #[cfg(test)]
    fn with_logo_cache_and_file(
        logo_cache: TokenLogoCache,
        logo_lists: TokenLogoListIndex,
        metadata_file: impl Into<PathBuf>,
        initial: HashMap<String, TokenMetadata>,
    ) -> Self {
        Self {
            cache: Arc::new(RwLock::new(initial)),
            logo_cache: Arc::new(logo_cache),
            logo_lists: Arc::new(logo_lists),
            metadata_file: metadata_file.into(),
        }
    }

    pub async fn get(&self, contract: &str) -> Option<TokenMetadata> {
        self.cache.read().await.get(contract).cloned()
    }

    pub async fn get_all(&self) -> HashMap<String, TokenMetadata> {
        self.cache.read().await.clone()
    }

    pub async fn replace_all(&self, tokens: HashMap<String, TokenMetadata>) {
        *self.cache.write().await = tokens;
    }

    pub async fn ensure_self_hosted_logos(&self) -> usize {
        let list_count = self.logo_lists.refresh().await;
        info!(list_count, "Refreshed SEP-42 logo index");

        let entries: Vec<(String, TokenMetadata)> = {
            let cache = self.cache.read().await;
            cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        };

        let mut success = 0usize;
        let mut official = 0usize;
        let mut updates: Vec<(String, String, LogoKind)> = Vec::new();

        for (id, meta) in entries {
            let list_icon = self.logo_lists.icon_url(&meta.contract).await;
            let remote = list_icon
                .as_deref()
                .or(meta.logo.as_deref().filter(|u| !u.contains("/logos/")));

            match self.logo_cache.ensure_logo(&meta.contract, &meta.symbol, remote).await {
                Ok(url) => {
                    success += 1;
                    let kind = if self.logo_cache.has_official_cache(&meta.contract) {
                        official += 1;
                        LogoKind::Official
                    } else {
                        LogoKind::Fallback
                    };
                    if meta.logo.as_deref() != Some(url.as_str()) || meta.logo_kind != Some(kind) {
                        updates.push((id, url, kind));
                    }
                }
                Err(e) => {
                    warn!("Failed to ensure self-hosted logo for {}: {}", id, e);
                }
            }
        }

        if !updates.is_empty() {
            {
                let mut cache = self.cache.write().await;
                for (id, url, kind) in updates {
                    if let Some(entry) = cache.get_mut(&id) {
                        entry.logo = Some(url);
                        entry.logo_kind = Some(kind);
                    }
                }
            }
            self.save().await;
        }

        info!(success, official, "Self-hosted logo enrichment complete");
        success
    }

    /// Resolve unknown tokens — Chakra stub (no Stellar RPC).
    pub async fn resolve_unknown(&self, _token_addresses: Vec<String>) {
        self.ensure_self_hosted_logos().await;
    }

    async fn save(&self) {
        let cache = self.cache.read().await;
        let file_cache = MetadataCache { tokens: cache.clone() };

        match serde_json::to_string_pretty(&file_cache) {
            Ok(json) => {
                if let Some(parent) = self.metadata_file.parent() {
                    if !parent.as_os_str().is_empty() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                }
                if let Err(e) = std::fs::write(&self.metadata_file, json) {
                    warn!("Failed to save token metadata: {}", e);
                } else {
                    info!("Saved {} token metadata entries to cache", cache.len());
                }
            }
            Err(e) => warn!("Failed to serialize token metadata: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{token_logo::TokenLogoCache, token_logo_lists::TokenLogoListIndex},
        std::{
            path::PathBuf,
            time::{SystemTime, UNIX_EPOCH},
        },
    };

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dex-adapters-token-meta-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn store_with_initial(
        logo_dir: &std::path::Path,
        base_url: &str,
        metadata_file: PathBuf,
        initial: HashMap<String, TokenMetadata>,
    ) -> TokenMetadataStore {
        TokenMetadataStore::with_logo_cache_and_file(
            TokenLogoCache::new(logo_dir, base_url),
            TokenLogoListIndex::new(vec![]),
            metadata_file,
            initial,
        )
    }

    #[tokio::test]
    async fn replace_all_overwrites_existing_cache() {
        let logo_dir = unique_temp_dir("replace-logos");
        let meta_file = unique_temp_dir("replace-meta").join("token_metadata.json");
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file, HashMap::new());
        let mut replacement = HashMap::new();
        replacement.insert(
            "token-1".to_string(),
            TokenMetadata {
                contract: "token-1".to_string(),
                symbol: "TOK".to_string(),
                name: "Token".to_string(),
                logo: None,
                logo_kind: None,
            },
        );

        store.replace_all(replacement).await;

        let all = store.get_all().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all["token-1"].symbol, "TOK");
    }

    #[tokio::test]
    async fn enriches_missing_logo_with_self_hosted_fallback() {
        let logo_dir = unique_temp_dir("enrich-logos");
        let meta_file = unique_temp_dir("enrich-meta").join("token_metadata.json");
        let mut initial = HashMap::new();
        initial.insert(
            "token-1".to_string(),
            TokenMetadata {
                contract: "token-1".to_string(),
                symbol: "TOK".to_string(),
                name: "Token".to_string(),
                logo: None,
                logo_kind: None,
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file.clone(), initial);

        let count = store.ensure_self_hosted_logos().await;

        let meta = store.get("token-1").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(meta.logo_kind, Some(LogoKind::Fallback));
        assert_eq!(std::fs::read_dir(&logo_dir).unwrap().count(), 1);
        assert_eq!(count, 1);

        let persisted = std::fs::read_to_string(&meta_file).expect("metadata persisted");
        assert!(persisted.contains("https://api.test/logos/"));
        assert!(persisted.contains("fallback"));
    }

    #[tokio::test]
    async fn resolve_unknown_backfills_logos_when_no_unknown_tokens() {
        let logo_dir = unique_temp_dir("backfill-logos");
        let meta_file = unique_temp_dir("backfill-meta").join("token_metadata.json");
        let mut initial = HashMap::new();
        initial.insert(
            "token-1".to_string(),
            TokenMetadata {
                contract: "token-1".to_string(),
                symbol: "TOK".to_string(),
                name: "Token".to_string(),
                logo: None,
                logo_kind: None,
            },
        );
        let store = store_with_initial(&logo_dir, "https://api.test/logos", meta_file, initial);

        store.resolve_unknown(vec!["token-1".to_string()]).await;

        let meta = store.get("token-1").await.expect("token present");
        assert!(meta.logo.as_deref().unwrap().starts_with("https://api.test/logos/"));
        assert_eq!(std::fs::read_dir(&logo_dir).unwrap().count(), 1);
    }
}
