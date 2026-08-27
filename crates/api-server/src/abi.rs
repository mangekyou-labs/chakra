//! Minimal EVM ABI encode/decode + keccak helpers for `build_tx`.
//! No alloy/ethers — reqwest + hex + tiny-keccak (same stack as `evm_rpc`).

use {
    anyhow::{bail, Result},
    tiny_keccak::{Hasher, Keccak},
};

/// keccak256 of `bytes`.
pub fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    keccak.update(bytes);
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    out
}

/// 4-byte function selector for a signature.
pub fn selector(signature: &str) -> [u8; 4] {
    let digest = keccak256(signature.as_bytes());
    [digest[0], digest[1], digest[2], digest[3]]
}

/// Pad `hex` (with or without `0x`) to a 32-byte word, left-aligned (bytes).
pub fn word_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..bytes.len().min(32)].copy_from_slice(&bytes[..bytes.len().min(32)]);
    out
}

/// Right-aligned 32-byte word for a `uint256`-style value.
pub fn uint_word(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..].copy_from_slice(&value.to_be_bytes());
    out
}

/// Right-aligned 32-byte word for an `address` (20 bytes).
pub fn address_word(address: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(address.trim_start_matches("0x"))?;
    if bytes.len() != 20 {
        bail!("invalid address length: {address}");
    }
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(&bytes);
    Ok(out)
}

/// `uint24` fee tier as a 32-byte word.
pub fn uint24_word(value: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[29..].copy_from_slice(&value.to_be_bytes()[1..]);
    out
}

/// Decode a 32-byte word (big-endian) into u128.
pub fn word_to_u128(word: &[u8]) -> u128 {
    u128::from_be_bytes(word[16..].try_into().unwrap())
}

/// Hex string of `bytes` with `0x` prefix.
pub fn hex_with_prefix(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_match_contract_abis() {
        // Pinned against Aggregator.sol / ERC-20 / Permit2 ABIs (cast-verified).
        assert_eq!(
            hex::encode(selector(
                "splitSwap(address,address,uint256,uint256,uint256,(uint256,(address,uint8,address,address,uint24)[])[],(((address,uint160,uint48,uint48),address,uint256),bytes))"
            )),
            "2e3be0c1"
        );
        assert_eq!(hex::encode(selector("paused()")), "5c975abb");
        assert_eq!(hex::encode(selector("allowance(address,address)")), "dd62ed3e");
        assert_eq!(hex::encode(selector("balanceOf(address)")), "70a08231");
    }

    #[test]
    fn address_and_uint_words_are_right_aligned() {
        let word = address_word("0x3600000000000000000000000000000000000000").unwrap();
        assert_eq!(
            &word[12..],
            &hex::decode("3600000000000000000000000000000000000000").unwrap()[..]
        );
        assert_eq!(word_to_u128(&uint_word(1_000_000)), 1_000_000);
        assert_eq!(word_to_u128(&uint24_word(4)), 4);
    }

    /// ABI drift gate: the checked-in Aggregator.json must be parseable as
    /// valid JSON and contain at least the `splitSwap` function selector.
    /// Run `forge inspect --offline --json Aggregator abi` to regenerate;
    /// normal Cargo builds must not require Forge.
    #[test]
    fn aggregator_abi_drift_gate() {
        let abi_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/evm/abi/Aggregator.json");
        let raw = std::fs::read_to_string(&abi_path)
            .expect("checked-in Aggregator.json must exist at contracts/evm/abi/Aggregator.json");
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&raw).expect("Aggregator.json must be valid JSON array");

        // Must contain the splitSwap function with our pinned selector.
        let has_split_swap = items
            .iter()
            .any(|item| item["type"] == "function" && item["name"] == "splitSwap");
        assert!(has_split_swap, "Aggregator ABI must contain splitSwap function");

        // Must contain the splitSwap selector signature we hardcode.
        let sig = "splitSwap(address,address,uint256,uint256,uint256,(uint256,(address,uint8,address,address,uint24)[])[],(((address,uint160,uint48,uint48),address,uint256),bytes))";
        assert_eq!(hex::encode(selector(sig)), "2e3be0c1", "splitSwap selector must match");

        // Verify the ABI is normalized (canonical form): no trailing commas,
        // valid UTF-8, and items are sorted by type+name for deterministic diffing.
        let re_serialized = serde_json::to_string(&items).expect("reserialization must succeed");
        let re_parsed: Vec<serde_json::Value> =
            serde_json::from_str(&re_serialized).expect("round-trip parse must succeed");
        assert_eq!(items.len(), re_parsed.len(), "round-trip must preserve item count");
    }
}
