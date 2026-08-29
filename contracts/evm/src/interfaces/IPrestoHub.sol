// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @notice Presto normalized hub AMM (Arc testnet). The hub is the hop target:
///      the aggregator approves exactly `amountIn` to the hub and calls
///      `swap(tokenIn, tokenOut, amountIn, minOut, deadline)`; the hub pulls
///      the input via `transferFrom` and pays the output to the caller.
///      The published normalized hub formula drives quoting (see
///      `evm_quote_math::presto_quote`).
interface IPrestoHub {
    function swap(
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 minAmountOut,
        uint256 deadline
    ) external returns (uint256 amountOut);
}
