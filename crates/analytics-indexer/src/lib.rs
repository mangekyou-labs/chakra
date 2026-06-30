//! On-chain analytics indexer for Chakra aggregator contract invocations.

pub mod config;
pub mod export;
pub mod ingest;
pub mod parser;
pub mod store;

pub use config::IndexerConfig;
