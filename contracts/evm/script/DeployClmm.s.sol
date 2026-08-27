// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";

/// @notice Deploy Uniswap V3 CLMM factory on Arc testnet.
/// @dev Uses VendorDeployer to deploy pre-compiled 0.7.6 bytecode.
///      Do not broadcast without PRIVATE_KEY in gitignored env.
contract DeployClmm is Script, VendorDeployer {
    function run() external {
        vm.startBroadcast();
        address factoryAddr = _deployFromHexFile("bytecodes/v3-factory.hex");
        vm.stopBroadcast();
        console.log("V3 Factory", factoryAddr);
    }
}
