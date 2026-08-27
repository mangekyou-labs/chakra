// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";

/// @notice Deploy Uniswap V2 xy=k factory on Arc testnet.
/// @dev Uses VendorDeployer to deploy pre-compiled 0.5.16 bytecode.
///      Do not broadcast without PRIVATE_KEY in gitignored env.
contract DeployXyk is Script, VendorDeployer {
    function run() external {
        vm.startBroadcast();
        address factoryAddr =
            _deployFromHexFileWithArgs("bytecodes/v2-factory.hex", abi.encode(address(0)));
        vm.stopBroadcast();
        console.log("V2 Factory", factoryAddr);
    }
}
