// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

interface IBlocklist {
    function isBlocklisted(address) external view returns (bool);
}

interface IBurn {
    function transfer(address, address, uint256) external returns (bool);
}

/// @notice Funds the disposable T9.4 QA wallet from the operator.
/// @dev Transfers ERC-20 USDC (6 dp) and a small native USDC (18 dp) gas buffer
///      to the QA address. Run via scripts/arc-operator.sh (key from env, never argv).
///      QA address must be set via CHAKRA_QA_WALLET env.
contract FundQaWallet is Script {
    uint256 internal constant CHAKRA_CHAIN_ID = 5042002;

    // Public Arc testnet catalog addresses + system precompile stubs.
    address internal constant USDC = 0x3600000000000000000000000000000000000000;
    address internal constant BLOCKLIST = 0x1800000000000000000000000000000000000001;
    address internal constant BURN_SYSTEM = 0x1800000000000000000000000000000000000000;
    address internal constant USDC_IMPL = 0xC6AD664ac6679F4Ce74e10E91449C93Ec1ae3cA6;

    function run() external {
        require(
            block.chainid == CHAKRA_CHAIN_ID, "FundQaWallet: chain must be Arc testnet (5042002)"
        );

        // Mock Arc testnet system precompiles for local forge simulation only.
        vm.mockCall(BLOCKLIST, abi.encodeWithSignature("isBlocklisted(address)"), abi.encode(false));
        vm.mockCall(
            BURN_SYSTEM,
            abi.encodeWithSignature("transfer(address,address,uint256)"),
            abi.encode(true)
        );

        address qa = vm.envAddress("CHAKRA_QA_WALLET");
        require(qa != address(0), "CHAKRA_QA_WALLET required");

        uint256 usdcAmount = vm.envOr("CHAKRA_QA_USDC", uint256(2_000_000)); // 2 USDC (6 dp)
        uint256 nativeGas = vm.envOr("CHAKRA_QA_NATIVE_GAS", uint256(1_000_000_000_000_000_000)); // 1 native USDC (18 dp)

        uint256 pk = vm.envUint("PRIVATE_KEY");
        address operator = vm.addr(pk);

        console.log("=== T9.4 QA Wallet Funding ===");
        console.log("Operator:", operator);
        console.log("QA wallet:", qa);
        console.log("ERC-20 USDC to send:", usdcAmount);
        console.log("Native USDC gas to send:", nativeGas);

        uint256 usdcBal = IERC20(USDC).balanceOf(operator);
        require(usdcBal >= usdcAmount, "Insufficient ERC-20 USDC balance");
        require(address(operator).balance >= nativeGas, "Insufficient native USDC balance");

        vm.startBroadcast(pk);

        if (usdcAmount > 0) {
            IERC20(USDC).transfer(qa, usdcAmount);
            console.log("ERC-20 USDC transferred:", usdcAmount);
        }
        if (nativeGas > 0) {
            (bool ok,) = qa.call{value: nativeGas}("");
            require(ok, "Native transfer failed");
            console.log("Native USDC transferred:", nativeGas);
        }

        vm.stopBroadcast();

        console.log("QA ERC-20 USDC balance:", IERC20(USDC).balanceOf(qa));
        console.log("QA native USDC balance:", qa.balance);
    }
}
