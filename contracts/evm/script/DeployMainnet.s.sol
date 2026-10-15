// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {Aggregator} from "../src/Aggregator.sol";

/// @notice Deploy Chakra on Arc mainnet and allowlist UnitFlow's standard V3
/// factory. No tokens, pools, liquidity, approvals or swaps are created.
contract DeployMainnet is Script {
    function run() external {
        require(block.chainid == 5042, "Arc mainnet only");
        address permit2 = vm.envAddress("CHAKRA_PERMIT2");
        address usdc = vm.envAddress("CHAKRA_USDC_ADDRESS");
        address eurc = vm.envAddress("CHAKRA_EURC_ADDRESS");
        address cirbtc = vm.envAddress("CHAKRA_CIRBTC_ADDRESS");
        address factory = vm.envAddress("CHAKRA_UNITFLOW_FACTORY");
        require(permit2.code.length > 0 && usdc.code.length > 0, "missing predeploy");
        require(eurc.code.length > 0 && cirbtc.code.length > 0, "missing token");
        require(factory.code.length > 0, "missing V3 factory");
        uint256 key = vm.envUint("OPERATOR_PRIVATE_KEY");
        console.log("Operator", vm.addr(key));
        console.log("Operator native USDC wei", vm.addr(key).balance);
        vm.startBroadcast(key);
        Aggregator aggregator = new Aggregator(permit2, usdc, eurc, cirbtc);
        aggregator.addFactory(factory, Aggregator.DexType.Clmm);
        vm.stopBroadcast();
        console.log("Aggregator", address(aggregator));
        console.log("UnitFlow V3 factory", factory);
    }
}
