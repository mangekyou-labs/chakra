// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {StableSwapFactory} from "../src/stable/StableSwapFactory.sol";

/// @notice Deploy StableSwap factory on Arc testnet.
/// @dev Do not broadcast without PRIVATE_KEY in gitignored env.
contract DeployStable is Script {
    function run() external {
        vm.startBroadcast();
        StableSwapFactory factory = new StableSwapFactory();
        vm.stopBroadcast();
        console.log("StableSwapFactory", address(factory));
    }
}
