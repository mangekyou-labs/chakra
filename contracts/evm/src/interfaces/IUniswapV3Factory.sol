// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @notice Minimal 0.8.30 ABI-only interface for UniswapV3Factory.
interface IUniswapV3Factory {
    event PoolCreated(
        address indexed token0,
        address indexed token1,
        uint24 indexed fee,
        int24 tickSpacing,
        address pool
    );

    function getPool(address tokenA, address tokenB, uint24 fee)
        external
        view
        returns (address pool);
    function createPool(address tokenA, address tokenB, uint24 fee) external returns (address pool);
    function feeAmountTickSpacing(uint24 fee) external view returns (int24);
}
