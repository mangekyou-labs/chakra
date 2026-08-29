// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {MockErc20} from "./MockErc20.sol";

/// @notice Presto hub test double mirroring the live `ArcHubAMMNormalized`
///      (`0x5794a8284A29493871Fbfa3c4f343D42001424D6`): a USDC path hub with
///      the published normalized formula (normalize → 997/1000 → denormalize,
///      two-leg pathUSD routing for non-USD pairs). `swap` pulls via
///      `transferFrom` and enforces its own deadline.
/// @dev The 6 dp tokens in the Foundry fixtures make the 18 dp normalization
///      a pure 1e12 scale (no precision loss); the Rust parity test pins the
///      same formula with arbitrary decimals.
contract MockPrestoHub {
    address public immutable pathUSD;
    uint8 public immutable pathUSDDecimals;

    // Raw reserves per user token (raw units) and per-token path (USDC) reserve.
    mapping(address => uint256) public tokenReserves;
    mapping(address => uint256) public pathReserves;

    uint256 public swapCount;

    error Expired();
    error SameToken();
    error ZeroAmount();
    error WrongPair();

    constructor(address _pathUSD) {
        pathUSD = _pathUSD;
        pathUSDDecimals = MockErc20(_pathUSD).decimals();
    }

    function decimalsOf(address token) public view returns (uint8) {
        return MockErc20(token).decimals();
    }

    /// @dev Seed a spoke with both reserves (caller must have minted + approved).
    function seedPair(address userToken, uint256 userAmount, uint256 pathAmount) external {
        MockErc20(userToken).transferFrom(msg.sender, address(this), userAmount);
        MockErc20(pathUSD).transferFrom(msg.sender, address(this), pathAmount);
        tokenReserves[userToken] += userAmount;
        pathReserves[userToken] += pathAmount;
    }

    /// @dev 997/1000 on normalized balances; exact port of `_getAmountOut`.
    function getQuote(address tokenIn, address tokenOut, uint256 amountIn)
        public
        view
        returns (uint256 amountOut)
    {
        if (amountIn == 0) return 0;
        if (tokenIn == tokenOut) return amountIn;

        if (tokenIn == pathUSD) {
            uint8 outDec = decimalsOf(tokenOut);
            uint256 inNorm = _normalize(amountIn, pathUSDDecimals);
            uint256 rIn = _normalize(pathReserves[tokenOut], pathUSDDecimals);
            uint256 rOut = _normalize(tokenReserves[tokenOut], outDec);
            return _denormalize(_getAmountOut(inNorm, rIn, rOut), outDec);
        }
        if (tokenOut == pathUSD) {
            uint8 inDec = decimalsOf(tokenIn);
            uint256 inNorm = _normalize(amountIn, inDec);
            uint256 rIn = _normalize(tokenReserves[tokenIn], inDec);
            uint256 rOut = _normalize(pathReserves[tokenIn], pathUSDDecimals);
            return _denormalize(_getAmountOut(inNorm, rIn, rOut), pathUSDDecimals);
        }
        // Two-leg routing through pathUSD (exact published order).
        uint8 inDec = decimalsOf(tokenIn);
        uint8 outDec = decimalsOf(tokenOut);
        uint256 inNorm = _normalize(amountIn, inDec);
        uint256 leg1 = _getAmountOut(
            inNorm,
            _normalize(tokenReserves[tokenIn], inDec),
            _normalize(pathReserves[tokenIn], pathUSDDecimals)
        );
        uint256 pathRaw = _denormalize(leg1, pathUSDDecimals);
        uint256 pathRounded = _normalize(pathRaw, pathUSDDecimals);
        uint256 leg2 = _getAmountOut(
            pathRounded,
            _normalize(pathReserves[tokenOut], pathUSDDecimals),
            _normalize(tokenReserves[tokenOut], outDec)
        );
        return _denormalize(leg2, outDec);
    }

    function swap(
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 minAmountOut,
        uint256 deadline
    ) external returns (uint256 amountOut) {
        if (block.timestamp > deadline) revert Expired();
        if (tokenIn == tokenOut) revert SameToken();
        if (amountIn == 0) revert ZeroAmount();

        MockErc20(tokenIn).transferFrom(msg.sender, address(this), amountIn);
        amountOut = getQuote(tokenIn, tokenOut, amountIn);
        if (amountOut == 0) revert WrongPair();

        if (tokenIn == pathUSD) {
            pathReserves[tokenOut] += amountIn;
            tokenReserves[tokenOut] -= amountOut;
        } else if (tokenOut == pathUSD) {
            tokenReserves[tokenIn] += amountIn;
            pathReserves[tokenIn] -= amountOut;
        } else {
            uint256 pathOut = _denormalize(
                _getAmountOut(
                    _normalize(amountIn, decimalsOf(tokenIn)),
                    _normalize(tokenReserves[tokenIn], decimalsOf(tokenIn)),
                    _normalize(pathReserves[tokenIn], pathUSDDecimals)
                ),
                pathUSDDecimals
            );
            pathReserves[tokenIn] += amountIn;
            pathReserves[tokenOut] -= pathOut;
            tokenReserves[tokenOut] -= amountOut;
        }

        if (amountOut < minAmountOut) revert("Slippage tolerance exceeded");
        MockErc20(tokenOut).transfer(msg.sender, amountOut);
        swapCount++;
    }

    function _normalize(uint256 amount, uint8 decimals_) internal pure returns (uint256) {
        return amount * (10 ** (18 - decimals_));
    }

    function _denormalize(uint256 amount, uint8 decimals_) internal pure returns (uint256) {
        return amount / (10 ** (18 - decimals_));
    }

    function _getAmountOut(uint256 amountIn, uint256 reserveIn, uint256 reserveOut)
        internal
        pure
        returns (uint256)
    {
        if (reserveIn == 0 || reserveOut == 0) return 0;
        uint256 amountInWithFee = amountIn * 997;
        uint256 numerator = amountInWithFee * reserveOut;
        uint256 denominator = (reserveIn * 1000) + amountInWithFee;
        return numerator / denominator;
    }
}
