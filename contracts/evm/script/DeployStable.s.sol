// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {StableSwapFactory} from "../src/stable/StableSwapFactory.sol";

/// @notice Deploy StableSwap factory on Arc testnet.
/// @dev **FIXTURE-ONLY / HISTORICAL (2026-08-29 rebaseline)** — the Arc
///      operator workflow never deploys Chakra-owned factories; use the
///      canonical curated manifest venues. Chain-31337 local fixtures only.
///      Do not broadcast without PRIVATE_KEY in gitignored env.
contract DeployStable is Script {
    function run() external {
        require(block.chainid == 31337, "DeployStable: fixture script forbidden on Arc testnet (chain must be 31337)");
        vm.startBroadcast();
        StableSwapFactory factory = new StableSwapFactory();
        vm.stopBroadcast();
        console.log("StableSwapFactory", address(factory));
    }
}
