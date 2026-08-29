// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {Aggregator} from "../src/Aggregator.sol";

/// @notice Deploy one non-upgradeable Aggregator (cirBTC sweep) and register
///      the canonical curated venues (2026-08-29 rebaseline).
/// @dev Operator workflow is restricted to THIS script: one aggregator deploy
///      plus venue registration. It never deploys tokens, factories, pools, or
///      liquidity (Deploy.s.sol / Seed.s.sol are chain-31337 fixture scripts).
///      Broadcast on Arc testnet via scripts/arc-operator.sh — the key comes
///      from gitignored env, never `--private-key` on the CLI.
///
///      Env:
///        CHAKRA_PERMIT2            (default Arc predeploy)
///        CHAKRA_USDC_ADDRESS       (default frozen catalog USDC)
///        CHAKRA_EURC_ADDRESS       (default frozen catalog EURC)
///        CHAKRA_CIRBTC_ADDRESS     (default frozen canonical cirBTC)
///        CHAKRA_XYLO_FACTORY       XyloNet factory (address:xylo)
///        CHAKRA_XYLO_ROUTER        XyloNet router (atomic pair with the factory)
///        CHAKRA_PRESTO_HUB         Presto hub
///        CHAKRA_UNITFLOW_FACTORY   UnitFlow V2.5 factory (Xyk, 30 bps)
contract DeployAggregator is Script {
    function run() external {
        address permit2 =
            vm.envOr("CHAKRA_PERMIT2", address(0x000000000022D473030F116dDEE9F6B43aC78BA3));
        address usdc =
            vm.envOr("CHAKRA_USDC_ADDRESS", address(0x3600000000000000000000000000000000000000));
        address eurc =
            vm.envOr("CHAKRA_EURC_ADDRESS", address(0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a));
        address cirbtc =
            vm.envOr("CHAKRA_CIRBTC_ADDRESS", address(0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF));

        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        Aggregator agg = new Aggregator(permit2, usdc, eurc, cirbtc);

        // Venue registration (all owner-only; no-op when the env placeholder
        // is empty). No token/factory/pool/liquidity deployment here.
        address xyloFactory = vm.envOr("CHAKRA_XYLO_FACTORY", address(0));
        address xyloRouter = vm.envOr("CHAKRA_XYLO_ROUTER", address(0));
        if (xyloFactory != address(0)) {
            agg.addFactory(xyloFactory, Aggregator.DexType.Xylo);
            if (xyloRouter != address(0)) agg.setXyloRouter(xyloFactory, xyloRouter);
        }
        address prestoHub = vm.envOr("CHAKRA_PRESTO_HUB", address(0));
        if (prestoHub != address(0)) agg.addPrestoHub(prestoHub);
        address unitflowFactory = vm.envOr("CHAKRA_UNITFLOW_FACTORY", address(0));
        if (unitflowFactory != address(0)) {
            agg.addFactory(unitflowFactory, Aggregator.DexType.Xyk);
            // UnitFlow V2.5 = 30 bps per-factory fee.
            agg.setFactoryFee(unitflowFactory, 30);
        }
        vm.stopBroadcast();

        console.log("Aggregator", address(agg));
        console.log("Permit2", permit2);
        console.log("cirBTC", cirbtc);
        console.log("Xylo factory registered", xyloFactory != address(0));
        console.log("Xylo router configured", xyloRouter != address(0));
        console.log("Presto hub allowlisted", prestoHub != address(0));
        console.log("UnitFlow factory registered (30 bps)", unitflowFactory != address(0));
    }
}
