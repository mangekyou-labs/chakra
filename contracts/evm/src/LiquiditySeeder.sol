// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IUniswapV2Pair} from "./interfaces/IUniswapV2Pair.sol";
import {IUniswapV3Pool} from "./interfaces/IUniswapV3Pool.sol";

/// @notice Small operator helper that initializes V2 and V3 liquidity while
/// keeping token custody with the operator until the venue requests payment.
contract LiquiditySeeder {
    error InvalidPool();
    error InvalidRecipient();
    error CallbackNotExpected();
    error AmountExceeded();
    error TransferFailed();

    address private expectedPool;
    address private expectedPayer;
    uint256 private maxAmount0;
    uint256 private maxAmount1;

    function seedV2(
        address pair,
        address tokenA,
        address tokenB,
        uint256 amountA,
        uint256 amountB,
        address recipient
    ) external returns (uint256 liquidity) {
        if (recipient == address(0)) revert InvalidRecipient();
        IUniswapV2Pair venue = IUniswapV2Pair(pair);
        address token0 = venue.token0();
        address token1 = venue.token1();
        if (!((tokenA == token0 && tokenB == token1) || (tokenA == token1 && tokenB == token0))) {
            revert InvalidPool();
        }
        _transferFrom(tokenA, msg.sender, pair, amountA);
        _transferFrom(tokenB, msg.sender, pair, amountB);
        liquidity = venue.mint(recipient);
    }

    function seedV3(
        address pool,
        int24 tickLower,
        int24 tickUpper,
        uint128 liquidity,
        uint256 amount0Max,
        uint256 amount1Max,
        address recipient
    ) external returns (uint256 amount0, uint256 amount1) {
        if (recipient == address(0)) revert InvalidRecipient();
        if (expectedPool != address(0)) revert CallbackNotExpected();

        expectedPool = pool;
        expectedPayer = msg.sender;
        maxAmount0 = amount0Max;
        maxAmount1 = amount1Max;
        (amount0, amount1) =
            IUniswapV3Pool(pool).mint(recipient, tickLower, tickUpper, liquidity, bytes(""));
        expectedPool = address(0);
        expectedPayer = address(0);
        maxAmount0 = 0;
        maxAmount1 = 0;
    }

    function uniswapV3MintCallback(uint256 amount0Owed, uint256 amount1Owed, bytes calldata)
        external
    {
        address pool = expectedPool;
        address payer = expectedPayer;
        if (pool == address(0) || msg.sender != pool || payer == address(0)) {
            revert CallbackNotExpected();
        }
        if (amount0Owed > maxAmount0 || amount1Owed > maxAmount1) revert AmountExceeded();

        IUniswapV3Pool venue = IUniswapV3Pool(pool);
        if (amount0Owed != 0) _transferFrom(venue.token0(), payer, pool, amount0Owed);
        if (amount1Owed != 0) _transferFrom(venue.token1(), payer, pool, amount1Owed);
    }

    function _transferFrom(address token, address from, address to, uint256 amount) private {
        if (!IERC20(token).transferFrom(from, to, amount)) revert TransferFailed();
    }
}
