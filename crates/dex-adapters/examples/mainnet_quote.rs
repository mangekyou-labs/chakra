//! Read-only mainnet integration check. Run with ARC_RPC_URL in the environment.
use anyhow::{bail, Result};
use dex_adapters::{clmm_math, evm_fetch::*, evm_logs::function_selector_hex, evm_rpc::EvmRpcClient};
use market_snapshot::{
    decimals::{CIRBTC, EURC, USDC_ERC20},
    ClmmPoolRefSnapshot,
};

#[tokio::main]
async fn main() -> Result<()> {
    let client = EvmRpcClient::single(&std::env::var("ARC_RPC_URL")?)?;
    let factory = "0x5bfBCeb73d39F722B1cB83fD2F11736b28c1Be6d";
    let quoter = "0x5AF6E89F0960Ff375AF84d9911D8153ef6240E34";
    for (a, b, fee, spacing) in [
        (USDC_ERC20, EURC, 100, 1),
        (USDC_ERC20, CIRBTC, 500, 10),
        (EURC, CIRBTC, 500, 10),
    ] {
        let pool = factory_has_clmm_pool(&client, factory, a, b, fee)
            .await?
            .expect("mainnet pool");
        let (t0, t1) = if a.to_ascii_lowercase() < b.to_ascii_lowercase() {
            (a, b)
        } else {
            (b, a)
        };
        let state = fetch_clmm_snapshot(
            &client,
            "chakra-clmm",
            &ClmmPoolRefSnapshot {
                source: "chakra-clmm".into(),
                pool_address: pool.clone(),
                token0: t0.into(),
                token1: t1.into(),
                fee_bps: fee / 100,
                tick_spacing: spacing,
                factory: factory.into(),
            },
        )
        .await?;
        let (local, ticks) = clmm_math::clmm_pool_from_snapshot(&state);
        let coverage = clmm_math::ClmmCoverageInput::from_snapshot(&local, state.coverage.as_ref().unwrap());
        for (input, output) in [(a, b), (b, a)] {
            let amount = if input == CIRBTC { 10 } else { 10_000 };
            let zero_for_one = input.eq_ignore_ascii_case(t0);
            if !clmm_math::clmm_swap_allowed(&local, &ticks, amount, zero_for_one, &coverage) {
                bail!(
                    "unusable mainnet coverage: {pool} tick={} ticks={:?} simulation={:?}",
                    state.tick,
                    state.ticks,
                    clmm_math::simulate_swap(&local, &ticks, amount, zero_for_one)
                );
            }
            let (out, _, _) = clmm_math::simulate_swap(&local, &ticks, amount, zero_for_one).expect("local quote");
            // UnitFlow exposes the original standard V3 Quoter ABI.
            let data = calldata(
                &function_selector_hex("quoteExactInputSingle(address,address,uint24,uint256,uint160)"),
                &[
                    encode_address_arg(input)?,
                    encode_address_arg(output)?,
                    encode_uint_arg(fee.into()),
                    encode_uint_arg(amount),
                    encode_uint_arg(0),
                ],
            );
            let response = client.eth_call(quoter, &data).await?;
            let words = split_words(&response)?;
            let onchain = u128::from_be_bytes(words[0][16..].try_into()?);
            println!(
                "{input} -> {output}: local={out}, quoter={onchain}, ticks={}",
                state.ticks.len()
            );
            // Live blocks can move between reads; reject a material difference.
            if out.abs_diff(onchain) > (onchain / 1000).max(2) {
                bail!("local/mainnet quote mismatch");
            }
        }
    }
    Ok(())
}
