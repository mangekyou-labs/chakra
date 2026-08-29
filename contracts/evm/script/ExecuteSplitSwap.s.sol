// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {StdCheats} from "forge-std/StdCheats.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IAllowanceTransfer} from "../src/interfaces/IAllowanceTransfer.sol";

/// @notice Executes splitSwap using the exact fresh calldata produced by the Chakra API /build_tx.
/// @dev Sets up prerequisite ERC-20 and Permit2 allowances on-chain, verifies value == 0,
///      and executes the transaction payload against the deployed Aggregator.
contract ExecuteSplitSwap is Script, StdCheats {
    uint256 internal constant CHAKRA_CHAIN_ID = 5042002;

    address internal constant USDC = 0x3600000000000000000000000000000000000000;
    address internal constant EURC = 0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a;
    address internal constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;
    address internal constant DEFAULT_AGGREGATOR = 0xEa1b2C24bd41163590960F8e40afe6cb4CC92006;

    function run() external {
        require(
            block.chainid == CHAKRA_CHAIN_ID,
            "ExecuteSplitSwap: chain must be Arc testnet (5042002)"
        );

        uint256 pk = vm.envUint("PRIVATE_KEY");
        address operator = vm.addr(pk);

        address aggregator = vm.envOr("CHAKRA_AGGREGATOR", DEFAULT_AGGREGATOR);
        bytes memory swapCalldata = vm.envBytes("CHAKRA_SWAP_CALLDATA");
        require(swapCalldata.length >= 4, "CHAKRA_SWAP_CALLDATA cannot be empty");

        // Verify selector matches splitSwap: 0x2e3be0c1
        bytes4 selector = bytes4(swapCalldata);
        require(
            selector == bytes4(0x2e3be0c1),
            "Calldata selector mismatch: expected splitSwap (0x2e3be0c1)"
        );

        // Assert zero ETH value requirement
        uint256 callValue = vm.envOr("CHAKRA_CALL_VALUE", uint256(0));
        require(callValue == 0, "Aggregator requires msg.value == 0 (direct ETH rejected)");

        console.log("=== T9.3 On-Chain Split Swap Execution ===");
        console.log("Operator:", operator);
        console.log("Aggregator:", aggregator);
        console.log("Calldata length:", swapCalldata.length);

        // Mock Arc testnet system precompiles and proxy balance for local forge simulation
        vm.mockCall(
            address(0x1800000000000000000000000000000000000001),
            abi.encodeWithSignature("isBlocklisted(address)"),
            abi.encode(false)
        );
        vm.mockCall(
            address(0x1800000000000000000000000000000000000000),
            abi.encodeWithSignature("transfer(address,address,uint256)"),
            abi.encode(true)
        );
        uint256 requiredAmount = 5_000_000;
        address usdcImpl = 0xC6AD664ac6679F4Ce74e10E91449C93Ec1ae3cA6;
        vm.mockCall(
            usdcImpl,
            abi.encodeWithSelector(IERC20.balanceOf.selector, aggregator),
            abi.encode(requiredAmount)
        );
        vm.mockCall(
            USDC,
            abi.encodeWithSelector(IERC20.balanceOf.selector, aggregator),
            abi.encode(requiredAmount)
        );

        uint256 usdcBal = IERC20(USDC).balanceOf(operator);
        uint256 eurcBalBefore = IERC20(EURC).balanceOf(operator);
        console.log("Initial USDC balance:", usdcBal);
        console.log("Initial EURC balance:", eurcBalBefore);
        require(usdcBal >= requiredAmount, "Insufficient USDC balance for 5e6 swap");
        vm.startBroadcast(pk);

        // 1. Ensure ERC-20 allowance to Permit2
        uint256 currentErc20Allowance = IERC20(USDC).allowance(operator, PERMIT2);
        if (currentErc20Allowance < requiredAmount) {
            console.log("Setting ERC-20 approval for Permit2...");
            IERC20(USDC).approve(PERMIT2, type(uint256).max);
        }

        // 2. Ensure on-chain Permit2 allowance for Aggregator
        IAllowanceTransfer.Allowance memory p2 =
            IAllowanceTransfer(PERMIT2).allowance(operator, USDC, aggregator);
        if (p2.amount < requiredAmount || p2.expiration < block.timestamp + 120) {
            console.log("Setting on-chain Permit2 allowance for Aggregator...");
            IAllowanceTransfer(PERMIT2)
                .approve(USDC, aggregator, type(uint160).max, uint48(block.timestamp + 86400));
        }

        // 3. Execute the exact calldata from /build_tx with value: 0
        console.log("Broadcasting splitSwap calldata with msg.value = 0...");
        (bool success, bytes memory returnData) = aggregator.call{value: 0}(swapCalldata);
        require(success, string(abi.encodePacked("splitSwap execution failed: ", returnData)));

        vm.stopBroadcast();

        uint256 eurcBalAfter = IERC20(EURC).balanceOf(operator);
        console.log("Swap completed successfully!");
        console.log("Initial EURC:", eurcBalBefore);
        console.log("Final EURC:  ", eurcBalAfter);
        console.log("Net EURC gain:", eurcBalAfter - eurcBalBefore);

        require(eurcBalAfter > eurcBalBefore, "EURC balance did not increase after swap");
    }
}
