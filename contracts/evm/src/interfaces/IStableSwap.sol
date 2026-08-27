// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @notice Interface for a 2-token StableSwap pool (Curve-style).
interface IStableSwap {
    event LiquidityAdded(
        address indexed provider, uint256 amount0, uint256 amount1, uint256 lpAmount
    );
    event LiquidityRemoved(
        address indexed provider, uint256 amount0, uint256 amount1, uint256 lpAmount
    );
    event Swapped(
        address indexed tokenIn, address indexed tokenOut, uint256 amountIn, uint256 amountOut
    );

    function token0() external view returns (address);
    function token1() external view returns (address);
    function balance0() external view returns (uint256);
    function balance1() external view returns (uint256);
    function totalSupply() external view returns (uint256);

    /// @notice Swap tokens. Caller must have already transferred tokenIn to this pool.
    /// @param i Index of tokenIn (0 or 1).
    /// @param j Index of tokenOut (0 or 1).
    /// @param amount Amount of tokenIn already on the pool (after transfer).
    /// @param minDy Minimum amount of tokenOut to receive (revert if less).
    /// @return dy Amount of tokenOut transferred to msg.sender.
    function exchange(uint256 i, uint256 j, uint256 amount, uint256 minDy)
        external
        returns (uint256 dy);

    /// @notice Add liquidity by transferring both tokens to the pool first.
    /// @param amount0 Amount of token0 (as transferred) used as the base.
    /// @param amount1 Amount of token1 (as transferred) used as the base.
    /// @return lpAmount Amount of LP tokens minted.
    function seedLiquidity(uint256 amount0, uint256 amount1) external returns (uint256 lpAmount);

    /// @notice Remove liquidity by burning LP tokens.
    /// @return amount0 Amount of token0 returned.
    /// @return amount1 Amount of token1 returned.
    function removeLiquidity(uint256 lpAmount) external returns (uint256 amount0, uint256 amount1);

    /// @notice Get the StableSwap invariant D.
    function getD() external view returns (uint256);

    /// @notice Get the virtual price (D / totalSupply scaled to 18 dp).
    function getVirtualPrice() external view returns (uint256);
}
