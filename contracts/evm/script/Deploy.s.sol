// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {Aggregator} from "../src/Aggregator.sol";
import {MockBtc} from "../src/MockBtc.sol";
import {StableSwapFactory} from "../src/stable/StableSwapFactory.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";

/// @notice Unified rerun-aware deploy: mBTC → factories → aggregator → allowlist.
/// @dev **FIXTURE-ONLY / HISTORICAL (2026-08-29 rebaseline).** Deploys
///      Chakra-owned tokens/factories/pools — the Arc operator workflow no
///      longer does this. Chain-31337 local fixtures only. The operator
///      workflow is `DeployAggregator.s.sol` (one aggregator deploy + venue
///      registration) and nothing else.
/// @dev Reads existing addresses from env (CHAKRA_MBTC_ADDRESS,
///      CHAKRA_XYK_FACTORY, CHAKRA_STABLE_FACTORY, CHAKRA_CLMM_FACTORY,
///      CHAKRA_AGGREGATOR) — if non-empty and bytecode matches, the step is
///      skipped.  Chain ID must be 5042002 (Arc testnet).  Never pass
///      --private-key on the CLI; use a Foundry keystore or env.
contract Deploy is Script, VendorDeployer {
    uint256 internal constant CHAKRA_CHAIN_ID = 5042002;

    // Expected bytecode sizes (bytes) for reuse-or-deploy checks.
    // mBTC: MockBtc initcode; factories: vendored hex files.
    uint256 internal constant V2_FACTORY_CODE_SIZE = 963; // UniswapV2 bytecode
    uint256 internal constant V3_FACTORY_CODE_SIZE = 3219; // UniswapV3 bytecode

    function run() external {
        require(block.chainid == 31337, "Deploy: fixture script forbidden on Arc testnet (chain must be 31337)");

        console.log("=== Arc Testnet Deploy (rerun-aware) ===");
        console.log("Chain ID:", block.chainid);

        address deployer = vm.addr(vm.envUint("PRIVATE_KEY"));
        console.log("Deployer:", deployer);

        // ── 1. mBTC ──────────────────────────────────────────
        address mbtcAddr = vm.envOr("CHAKRA_MBTC_ADDRESS", address(0));
        if (mbtcAddr == address(0)) {
            console.log("[1/5] Deploying mBTC...");
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            MockBtc mbtc = new MockBtc();
            vm.stopBroadcast();
            mbtcAddr = address(mbtc);
            console.log("  mBTC deployed:", mbtcAddr);
        } else {
            require(_hasCode(mbtcAddr), "CHAKRA_MBTC_ADDRESS set but no code at address");
            console.log("[1/5] mBTC reused:", mbtcAddr);
        }

        // ── 2. V2/XYK Factory ────────────────────────────────
        address xykAddr = vm.envOr("CHAKRA_XYK_FACTORY", address(0));
        if (xykAddr == address(0)) {
            console.log("[2/5] Deploying V2/XYK factory...");
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            xykAddr = _deployFromHexFileWithArgs("bytecodes/v2-factory.hex", abi.encode(address(0)));
            vm.stopBroadcast();
            console.log("  V2 Factory deployed:", xykAddr);
        } else {
            require(_hasCode(xykAddr), "CHAKRA_XYK_FACTORY set but no code at address");
            console.log("[2/5] V2 Factory reused:", xykAddr);
        }

        // ── 3. Stable Factory ─────────────────────────────────
        address stableAddr = vm.envOr("CHAKRA_STABLE_FACTORY", address(0));
        if (stableAddr == address(0)) {
            console.log("[3/5] Deploying StableSwap factory...");
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            StableSwapFactory sf = new StableSwapFactory();
            vm.stopBroadcast();
            stableAddr = address(sf);
            console.log("  StableSwapFactory deployed:", stableAddr);
        } else {
            require(_hasCode(stableAddr), "CHAKRA_STABLE_FACTORY set but no code at address");
            console.log("[3/5] StableSwapFactory reused:", stableAddr);
        }

        // ── 4. V3/CLMM Factory ────────────────────────────────
        address clmmAddr = vm.envOr("CHAKRA_CLMM_FACTORY", address(0));
        if (clmmAddr == address(0)) {
            console.log("[4/5] Deploying V3/CLMM factory...");
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            clmmAddr = _deployFromHexFile("bytecodes/v3-factory.hex");
            vm.stopBroadcast();
            console.log("  V3 Factory deployed:", clmmAddr);
        } else {
            require(_hasCode(clmmAddr), "CHAKRA_CLMM_FACTORY set but no code at address");
            console.log("[4/5] V3 Factory reused:", clmmAddr);
        }

        // ── 5. Aggregator + allowlist ─────────────────────────
        address aggAddr = vm.envOr("CHAKRA_AGGREGATOR", address(0));
        address permit2 =
            vm.envOr("CHAKRA_PERMIT2", address(0x000000000022D473030F116dDEE9F6B43aC78BA3));
        address usdc =
            vm.envOr("CHAKRA_USDC_ADDRESS", address(0x3600000000000000000000000000000000000000));
        address eurc =
            vm.envOr("CHAKRA_EURC_ADDRESS", address(0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a));

        if (aggAddr == address(0)) {
            console.log("[5/5] Deploying Aggregator...");
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            Aggregator agg = new Aggregator(permit2, usdc, eurc, mbtcAddr);

            // Allowlist factories.
            agg.addFactory(xykAddr, Aggregator.DexType.Xyk);
            agg.addFactory(stableAddr, Aggregator.DexType.Stable);
            agg.addFactory(clmmAddr, Aggregator.DexType.Clmm);

            vm.stopBroadcast();
            aggAddr = address(agg);
            console.log("  Aggregator deployed:", aggAddr);
        } else {
            require(_hasCode(aggAddr), "CHAKRA_AGGREGATOR set but no code at address");
            console.log("[5/5] Aggregator reused:", aggAddr);
        }

        // ── Summary ───────────────────────────────────────────
        console.log("");
        console.log("=== Deployment Summary ===");
        console.log("Chain ID:    5042002");
        console.log("Deployer:   ", deployer);
        console.log("mBTC:       ", mbtcAddr);
        console.log("V2 Factory: ", xykAddr);
        console.log("Stable Fty: ", stableAddr);
        console.log("V3 Factory: ", clmmAddr);
        console.log("Aggregator: ", aggAddr);
        console.log("Permit2:    ", permit2);
        console.log("USDC:       ", usdc);
        console.log("EURC:       ", eurc);
        console.log("");
        console.log("Set these in .env for Seed.s.sol and the API/worker:");
        console.log("  CHAKRA_MBTC_ADDRESS=", vm.toString(mbtcAddr));
        console.log("  CHAKRA_XYK_FACTORY=", vm.toString(xykAddr));
        console.log("  CHAKRA_STABLE_FACTORY=", vm.toString(stableAddr));
        console.log("  CHAKRA_CLMM_FACTORY=", vm.toString(clmmAddr));
        console.log("  CHAKRA_AGGREGATOR=", vm.toString(aggAddr));
    }

    function _hasCode(address addr) internal view returns (bool) {
        uint256 size;
        assembly {
            size := extcodesize(addr)
        }
        return size > 0;
    }
}
