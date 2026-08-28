// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {MockErc20} from "./MockErc20.sol";

/// @notice Minimal XyloNet stableswap test double for the aggregator hop
///      tests. Mirrors the real venue's ABI: `swap(tokenIn, tokenOut,
///      amountIn, minOut, to, deadline)` pulls via `transferFrom`; fee taken
///      on output (4 bps); `getReserves`/`token0`/`token1` for the quote
///      math probes.
contract MockXyloPool {
    address public immutable token0;
    address public immutable token1;

    uint256 public reserve0;
    uint256 public reserve1;
    uint256 public swapCount;

    error InsufficientInput();
    error WrongPair();

    constructor(address _token0, address _token1, uint256 _r0, uint256 _r1) {
        token0 = _token0;
        token1 = _token1;
        reserve0 = _r0;
        reserve1 = _r1;
    }

    function getReserves() external view returns (uint256, uint256) {
        return (reserve0, reserve1);
    }

    /// @dev Xylo fee is taken on output: `dy = gross - gross * 4 / 10000`.
    function _dy(uint256 amountIn, uint256 rIn, uint256 rOut) internal pure returns (uint256) {
        uint256 gross = (amountIn * rOut) / (rIn + amountIn);
        return gross - (gross * 4) / 10000;
    }

    function swap(
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 minOut,
        address to,
        uint256 deadline
    ) external returns (uint256 amountOut) {
        if (block.timestamp > deadline) revert("Expired");
        uint256 rIn;
        uint256 rOut;
        if (tokenIn == token0 && tokenOut == token1) {
            (rIn, rOut) = (reserve0, reserve1);
        } else if (tokenIn == token1 && tokenOut == token0) {
            (rIn, rOut) = (reserve1, reserve0);
        } else {
            revert WrongPair();
        }
        amountOut = _dy(amountIn, rIn, rOut);
        if (amountOut < minOut) revert InsufficientInput();

        MockErc20(tokenIn).transferFrom(msg.sender, address(this), amountIn);
        MockErc20(tokenOut).transfer(to, amountOut);

        if (tokenIn == token0) {
            reserve0 += amountIn;
            reserve1 -= amountOut;
        } else {
            reserve1 += amountIn;
            reserve0 -= amountOut;
        }
        swapCount++;
    }
}

contract MockXyloFactory {
    mapping(address => mapping(address => address)) public pools;
    address[] public allPools;

    /// @dev The caller must have minted `r0`/`r1` of both tokens and
    ///      approved this factory. The factory pulls them in and forwards
    ///      them to the pool (the pool's constructor cannot pull from the
    ///      caller directly — its msg.sender during CREATE is itself).
    function createPool(address tokenA, address tokenB, uint256 r0, uint256 r1) external returns (address) {
        MockErc20(tokenA).transferFrom(msg.sender, address(this), r0);
        MockErc20(tokenB).transferFrom(msg.sender, address(this), r1);
        MockXyloPool pool = new MockXyloPool(tokenA, tokenB, r0, r1);
        MockErc20(tokenA).transfer(address(pool), r0);
        MockErc20(tokenB).transfer(address(pool), r1);
        pools[tokenA][tokenB] = address(pool);
        pools[tokenB][tokenA] = address(pool);
        allPools.push(address(pool));
        return address(pool);
    }

    function getPool(address tokenA, address tokenB) external view returns (address) {
        return pools[tokenA][tokenB];
    }

    function allPoolsLength() external view returns (uint256) {
        return allPools.length;
    }
}
