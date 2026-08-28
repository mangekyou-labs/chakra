// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {Aggregator} from "../src/Aggregator.sol";

/// @notice Deploy Aggregator (non-upgradeable) and allowlist seed factories.
/// @dev Compile-only in CI/local; operator broadcast on Arc testnet uses
///      gitignored env (never `--private-key` on the CLI). Permit2 defaults to
///      the Arc predeploy; USDC/EURC default to the frozen catalog addresses;
///      mBTC and the factory address placeholders come from CHAKRA_* env.
contract DeployAggregator is Script {
    function run() external {
        address permit2 =
            vm.envOr("CHAKRA_PERMIT2", address(0x000000000022D473030F116dDEE9F6B43aC78BA3));
        address usdc =
            vm.envOr("CHAKRA_USDC_ADDRESS", address(0x3600000000000000000000000000000000000000));
        address eurc =
            vm.envOr("CHAKRA_EURC_ADDRESS", address(0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a));
        address mbtc = vm.envAddress("CHAKRA_MBTC_ADDRESS");

        vm.startBroadcast();
        Aggregator agg = new Aggregator(permit2, usdc, eurc, mbtc);

        // Factories come from env placeholders; no auto-allowlist, no broadcasting
        // of admin calls when the placeholder is empty.
        address xyk = vm.envOr("CHAKRA_XYK_FACTORY", address(0));
        address stable = vm.envOr("CHAKRA_STABLE_FACTORY", address(0));
        address clmm = vm.envOr("CHAKRA_CLMM_FACTORY", address(0));
        // T-XYLO: XyloNet factory (pinned USDC/EURC pool only; the aggregator
        // verifies `getPool(tokenIn, tokenOut) == pool` per hop).
        address xylo = vm.envOr("CHAKRA_XYLO_FACTORY", address(0));
        if (xyk != address(0)) agg.addFactory(xyk, Aggregator.DexType.Xyk);
        if (stable != address(0)) agg.addFactory(stable, Aggregator.DexType.Stable);
        if (clmm != address(0)) agg.addFactory(clmm, Aggregator.DexType.Clmm);
        if (xylo != address(0)) agg.addFactory(xylo, Aggregator.DexType.Xylo);
        vm.stopBroadcast();

        console.log("Aggregator", address(agg));
        console.log("Permit2", permit2);
        console.log("Xyk factory allowlisted", xyk != address(0));
        console.log("Stable factory allowlisted", stable != address(0));
        console.log("Clmm factory allowlisted", clmm != address(0));
        console.log("Xylo factory allowlisted", xylo != address(0));
    }
}
