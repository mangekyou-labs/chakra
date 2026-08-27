// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @notice ABI-only interface for a stable-swap factory exposing Uniswap-style getPool.
/// @dev Both orderings return the same pool (StableSwapFactory stores tokenA/tokenB
///      and tokenB/tokenA). Lets the aggregator membership-check stable hops.
interface IStableSwapFactory {
    function getPool(address tokenA, address tokenB) external view returns (address pool);
}
