// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {MockBtc} from "../src/MockBtc.sol";

/// Deploy mBTC on Arc testnet. **FIXTURE-ONLY / HISTORICAL (2026-08-29
/// rebaseline)** — the Arc operator workflow never deploys mBTC; the
/// canonical catalog uses cirBTC. Chain-31337 local fixtures only.
/// Do not pass --private-key on the CLI in CI.
/// Use FOUNDRY_ETH_RPC_URL + a keystore / env loaded from gitignored .env.
contract DeployMockBtc is Script {
    function run() external {
        require(block.chainid == 31337, "DeployMockBtc: fixture script forbidden on Arc testnet (chain must be 31337)");
        vm.startBroadcast();
        MockBtc token = new MockBtc();
        vm.stopBroadcast();
        console.log("mBTC", address(token));
        console.log("decimals", token.decimals());
        console.log("owner", token.owner());
    }
}
