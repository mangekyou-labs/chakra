//! `/balances`: catalog ERC-20 `balanceOf` via Multicall3 `aggregate3` plus a
//! separate `native_usdc` (`eth_getBalance`, 18 dp). The two USDC encodings
//! are **never summed** (SC-12); the swap USDC field is the ERC-20 figure only.

use {
    crate::catalog,
    anyhow::{bail, Context, Result},
    dex_adapters::evm_rpc::EvmRpcClient,
    market_snapshot::decimals::{CatalogToken, NATIVE_USDC},
    serde_json::{json, Value},
};

/// Multicall3 predeploy on Arc testnet.
pub const MULTICALL3: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

/// `aggregate3((address,bytes,bool)[])` selector.
const AGGREGATE3_SELECTOR: &str = "0x82ad56cb";

/// `balanceOf(address)` selector.
const BALANCE_OF_SELECTOR: &str = "0x70a08231";

/// 32-byte word hex without `0x`.
fn encode_word(value: &str) -> String {
    let hex = value.trim_start_matches("0x");
    format!("{hex:0>64}")
}

fn encode_address(address: &str) -> String {
    let address = address.trim_start_matches("0x");
    format!("{address:0>40}")
}

fn encode_uint256(value: u128) -> String {
    format!("{value:0>64x}")
}

/// One `aggregate3` tuple: `(address target, bytes callData, bool allowFailure)`.
/// Head = target + offset(0x60) + allowFailure; tail = length + padded data.
fn aggregate3_entry(target: &str, call_data: &str) -> String {
    format!(
        "{}{}{}{}{}",
        encode_address(target),
        encode_uint256(96),
        encode_uint256(1),
        encode_uint256((call_data.len() / 2) as u128),
        encode_word(call_data),
    )
}

/// Encode one Multicall3 `aggregate3` batch of `balanceOf` calls.
fn aggregate3_balance_of_calldata(tokens: &[CatalogToken], account: &str) -> String {
    let mut calldata = String::from(AGGREGATE3_SELECTOR);
    calldata.push_str(&encode_uint256(32));
    calldata.push_str(&encode_uint256(tokens.len() as u128));
    for token in tokens {
        let call = format!("{BALANCE_OF_SELECTOR}{}", encode_address(account));
        calldata.push_str(&aggregate3_entry(&token.address, &call));
    }
    calldata
}

/// Fetch catalog ERC-20 balances via Multicall3 + a separate `native_usdc`.
/// Returns `{usdc, eurc, cirbtc, native_usdc}` with string values.
pub async fn fetch_balances(
    rpc: &EvmRpcClient,
    account: &str,
) -> Result<serde_json::Map<String, Value>> {
    let mut out = serde_json::Map::new();
    let account = account.to_ascii_lowercase();

    let tokens: Vec<CatalogToken> = catalog::catalog_swap_tokens();
    let calldata = aggregate3_balance_of_calldata(&tokens, &account);
    let result = rpc.eth_call(MULTICALL3, &calldata).await?;

    let bytes = dex_adapters::evm_rpc::decode_hex(&result).with_context(|| "aggregate3 result decode failed")?;
    // aggregate3 returns `(bool, bytes)[]`: [offset][length] then per element
    // head [success][dataOffset] + tail [dataLen][data].
    if bytes.len() < 64 {
        bail!("aggregate3 result truncated");
    }
    let count = u64::from_be_bytes(bytes[56..64].try_into().unwrap()) as usize;
    let mut cursor = 64usize;
    for (i, token) in tokens.iter().enumerate().take(count) {
        if bytes.len() < cursor + 64 {
            bail!("aggregate3 result truncated at element {i}");
        }
        let data_offset = u64::from_be_bytes(bytes[cursor + 56..cursor + 64].try_into().unwrap()) as usize;
        let data_start = cursor + data_offset; // points at [dataLen]
        if bytes.len() < data_start + 64 {
            bail!("aggregate3 result truncated at element {i} data");
        }
        let data_len = u64::from_be_bytes(bytes[data_start + 24..data_start + 32].try_into().unwrap()) as usize;
        let mut value_bytes = [0u8; 32];
        let value_start = data_start + 32;
        if bytes.len() < value_start + data_len {
            bail!("aggregate3 result truncated at element {i} value");
        }
        value_bytes[..data_len.min(32)].copy_from_slice(&bytes[value_start..value_start + data_len.min(32)]);
        let value = u128::from_be_bytes(value_bytes[16..].try_into().unwrap());
        out.insert(token.symbol.to_ascii_lowercase(), json!(value.to_string()));
        cursor += 64 + 32 + data_len.div_ceil(32) * 32;
    }

    // Native gas (18 dp) is a separate field; never summed with ERC-20 USDC.
    let wei_hex = rpc.eth_get_balance(&account).await?;
    let wei = parse_hex_u128(&wei_hex).unwrap_or(0);
    out.insert(NATIVE_USDC.to_string(), json!(wei.to_string()));
    Ok(out)
}

/// Parse `0x`-prefixed hex into u128 (native balances exceed u64::MAX).
/// Accepts odd-length hex (e.g. `0x55de6a779bbac0000`) by left-padding.
fn parse_hex_u128(hex: &str) -> Option<u128> {
    let hex = hex.trim_start_matches("0x");
    if hex.len() > 32 {
        return None;
    }
    let padded_hex;
    let hex = if hex.len() % 2 == 1 {
        padded_hex = format!("0{hex}");
        padded_hex.as_str()
    } else {
        hex
    };
    let mut padded = [0u8; 16];
    let bytes = hex::decode(hex).ok()?;
    if bytes.len() > 16 {
        return None;
    }
    padded[16 - bytes.len()..].copy_from_slice(&bytes);
    Some(u128::from_be_bytes(padded))
}
