// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";

/// @notice Deploy Uniswap V3 CLMM factory on Arc testnet.
/// @dev **FIXTURE-ONLY / HISTORICAL (2026-08-29 rebaseline)** — the Arc
///      operator workflow never deploys Chakra-owned factories; use the
///      canonical curated manifest venues. Chain-31337 local fixtures only.
/// @dev Uses VendorDeployer to deploy pre-compiled 0.7.6 bytecode.
///      Do not broadcast without PRIVATE_KEY in gitignored env.
contract DeployClmm is Script, VendorDeployer {
    function run() external {
        require(block.chainid == 31337, "DeployClmm: fixture script forbidden on Arc testnet (chain must be 31337)");
        vm.startBroadcast();
        address factoryAddr = _deployFromHexFile("bytecodes/v3-factory.hex");
        vm.stopBroadcast();
        console.log("V3 Factory", factoryAddr);
    }
}
