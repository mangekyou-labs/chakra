// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @notice Minimal 0.8.30 ABI-only interface for UniswapV2Factory.
/// Tests/scripts deploy V2 via `deployCode`; this interface lets 0.8.30 code talk to it.
interface IUniswapV2Factory {
    event PairCreated(address indexed token0, address indexed token1, address pair, uint256);

    function getPair(address tokenA, address tokenB) external view returns (address pair);
    function createPair(address tokenA, address tokenB) external returns (address pair);
}
