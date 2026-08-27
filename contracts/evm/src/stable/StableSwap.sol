// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {IERC20} from "../interfaces/IERC20Minimal.sol";

/// @title StableSwap — 2-token Curve-style stable swap pool.
/// @notice A = 100, fee = 4 bps, fee-on-input. Apache-2.0 original code.
/// @dev Math ported from crates/dex-adapters/src/stable_math.rs.
///      Fee is taken on input (before computing new y), matching the Rust get_dy.
///      exchange() does NOT call transferFrom: the aggregator pre-transfers
///      tokenIn, then calls exchange(i, j, amount, minDy).
contract StableSwap {
    uint256 public constant A = 100;
    uint256 public constant FEE_DENOMINATOR = 10_000;
    uint256 public constant FEE_BPS = 4;
    uint256 private constant N = 2;

    error ZeroAddress();
    error TokensMustDiffer();
    error SameIndex();
    error ZeroAmount();
    error InsufficientLiquidity();
    error SlippageExceeded(uint256 got, uint256 min);
    error IndexOutOfRange();
    error InsufficientInput();

    uint256 public reserve0;
    uint256 public reserve1;
    address public immutable token0;
    address public immutable token1;
    address public immutable factory;

    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;

    constructor(address _token0, address _token1, address _factory) {
        if (_token0 == address(0) || _token1 == address(0)) revert ZeroAddress();
        if (_token0 == _token1) revert TokensMustDiffer();
        token0 = _token0;
        token1 = _token1;
        factory = _factory;
    }

    function balance0() external view returns (uint256) {
        return IERC20(token0).balanceOf(address(this));
    }

    function balance1() external view returns (uint256) {
        return IERC20(token1).balanceOf(address(this));
    }

    function getD() public view returns (uint256) {
        uint256 b0 = IERC20(token0).balanceOf(address(this));
        uint256 b1 = IERC20(token1).balanceOf(address(this));
        if (b0 == 0 && b1 == 0) return 0;
        return _getD(b0, b1);
    }

    function getVirtualPrice() external view returns (uint256) {
        uint256 d = getD();
        if (totalSupply == 0) return 0;
        return (d * 1e18) / totalSupply;
    }

    // ─── Liquidity ─────────────────────────────────────────────

    function seedLiquidity(uint256 amount0, uint256 amount1) external returns (uint256 lpAmount) {
        if (amount0 == 0 || amount1 == 0) revert ZeroAmount();
        if (totalSupply != 0) revert("StableSwap: already seeded");

        uint256 b0 = IERC20(token0).balanceOf(address(this));
        uint256 b1 = IERC20(token1).balanceOf(address(this));
        require(b0 > 0 && b1 > 0, "StableSwap: no tokens");

        lpAmount = _sqrt(b0 * b1);
        if (lpAmount == 0) revert InsufficientLiquidity();

        totalSupply = lpAmount;
        reserve0 = b0;
        reserve1 = b1;
        balanceOf[msg.sender] = lpAmount;
        emit LiquidityAdded(msg.sender, b0, b1, lpAmount);
    }

    // ─── Swap ──────────────────────────────────────────────────

    /// @notice Swap. Caller must have already transferred tokenIn to this pool.
    /// @param i Index of tokenIn (0 or 1).
    /// @param j Index of tokenOut (0 or 1).
    /// @param amount Amount of tokenIn transferred to this pool (fee-inclusive).
    /// @param minDy Minimum acceptable output.
    function exchange(uint256 i, uint256 j, uint256 amount, uint256 minDy)
        external
        returns (uint256 dy)
    {
        if (i > 1 || j > 1) revert IndexOutOfRange();
        if (i == j) revert SameIndex();
        if (amount == 0) revert ZeroAmount();

        // Stored reserves (snapshot of last known state)
        uint256 storedIn = (i == 0) ? reserve0 : reserve1;

        // Actual input = current balance - stored reserve (must be > 0)
        uint256 currentBalIn = (i == 0)
            ? IERC20(token0).balanceOf(address(this))
            : IERC20(token1).balanceOf(address(this));
        uint256 actualIn = currentBalIn - storedIn;
        if (actualIn == 0) revert ZeroAmount();
        if (actualIn < amount) revert InsufficientInput();

        // Fee on input
        uint256 fee = (amount * FEE_BPS + FEE_DENOMINATOR - 1) / FEE_DENOMINATOR;
        uint256 amountAfterFee = amount - fee;

        // Old balance of token[i] (stored reserve before this swap deposit)
        uint256 oldBalI = storedIn;
        // Old balance of token[j] (stored reserve, unchanged by this swap)
        uint256 oldBalJ = (i == 0) ? reserve1 : reserve0;

        // x_new = old balance + fee-adjusted input
        uint256 xNew = oldBalI + amountAfterFee;

        // Compute output from old invariant
        dy = _getDyFromOld(oldBalI, oldBalJ, xNew);

        if (dy == 0 || dy < minDy) revert SlippageExceeded(dy, minDy);

        // Update stored reserves
        if (i == 0) {
            reserve0 = storedIn + amount;
            reserve1 = oldBalJ - dy;
        } else {
            reserve1 = storedIn + amount;
            reserve0 = oldBalJ - dy;
        }

        address outToken = i == 0 ? token1 : token0;
        IERC20(outToken).transfer(msg.sender, dy);
        emit Swapped(i == 0 ? token0 : token1, i == 0 ? token1 : token0, amount, dy);
    }

    function removeLiquidity(uint256 lpAmount) external returns (uint256 amount0, uint256 amount1) {
        if (lpAmount == 0) revert ZeroAmount();
        if (balanceOf[msg.sender] < lpAmount) revert InsufficientLiquidity();

        uint256 supply = totalSupply;
        amount0 = (lpAmount * reserve0) / supply;
        amount1 = (lpAmount * reserve1) / supply;

        balanceOf[msg.sender] -= lpAmount;
        totalSupply -= lpAmount;

        // Update stored reserves
        reserve0 -= amount0;
        reserve1 -= amount1;

        IERC20(token0).transfer(msg.sender, amount0);
        IERC20(token1).transfer(msg.sender, amount1);
        emit LiquidityRemoved(msg.sender, amount0, amount1, lpAmount);
    }

    // ─── Internal math (Curve invariant, equal-decimal 2-token) ─

    /// @dev StableSwap invariant D. Newton's method.
    function _getD(uint256 x0, uint256 x1) internal pure returns (uint256) {
        if (x0 == 0 && x1 == 0) return 0;
        uint256 ann = A * N; // A * n^n = 100 * 4 = 400
        uint256 s = x0 + x1;
        if (s == 0) return 0;

        uint256 d = s;
        for (uint256 iter = 0; iter < 255; iter++) {
            uint256 dPrev = d;
            // d_p = D^(n+1) / (n^n * prod(x_i)) for n=2: D^3 / (4 * x0 * x1)
            uint256 dP = d * d / (N * x0) * d / (N * x1);
            if (dP == 0) break;
            d = (ann * s + dP * N) * d / ((ann - 1) * d + (N + 1) * dP);
            if (d > dPrev) {
                if (d - dPrev <= 1) return d;
            } else {
                if (dPrev - d <= 1) return d;
            }
        }
        return d;
    }

    /// @dev Compute dy from OLD balances and x_new (fee-adjusted new balance of token[i]).
    function _getDyFromOld(uint256 oldBalI, uint256 oldBalJ, uint256 xNew)
        internal
        view
        returns (uint256)
    {
        // D from old (pre-swap) balances
        uint256 d = _getD(oldBalI, oldBalJ);
        if (d == 0) return 0;

        uint256 ann = A * N;

        // Newton: find y such that D(xNew, y) = d
        // c = D^3 / (4 * xNew * Ann), b = xNew + D/Ann
        uint256 c = d * d / (N * xNew) * d / (ann * N);
        uint256 b = xNew + d / ann;

        uint256 y = oldBalJ; // start from old balance
        for (uint256 iter = 0; iter < 255; iter++) {
            uint256 yPrev = y;
            // y = (c + y^2) / (2*y + b - D)
            y = (c + y * y) / (2 * y + b - d);
            if (y > yPrev) {
                if (y - yPrev <= 1) break;
            } else {
                if (yPrev - y <= 1) break;
            }
        }

        if (y >= oldBalJ) return 0;
        return oldBalJ - y - 1; // -1 for rounding safety
    }

    /// @dev Integer square root.
    function _sqrt(uint256 x) internal pure returns (uint256) {
        if (x == 0) return 0;
        uint256 z = (x + 1) / 2;
        uint256 y = x;
        while (z < y) {
            y = z;
            z = (x / z + z) / 2;
        }
        return y;
    }

    // ─── Events ─────────────────────────────────────────────────
    event LiquidityAdded(
        address indexed provider, uint256 amount0, uint256 amount1, uint256 lpAmount
    );
    event LiquidityRemoved(
        address indexed provider, uint256 amount0, uint256 amount1, uint256 lpAmount
    );
    event Swapped(
        address indexed tokenIn, address indexed tokenOut, uint256 amountIn, uint256 amountOut
    );
}
