// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";

import {IAllowanceTransfer} from "./interfaces/IAllowanceTransfer.sol";
import {IStableSwapFactory} from "./interfaces/IStableSwapFactory.sol";
import {IUniswapV2Factory} from "./interfaces/IUniswapV2Factory.sol";
import {IUniswapV2Pair} from "./interfaces/IUniswapV2Pair.sol";
import {IUniswapV3Factory} from "./interfaces/IUniswapV3Factory.sol";
import {IUniswapV3Pool} from "./interfaces/IUniswapV3Pool.sol";
import {IStableSwap} from "./interfaces/IStableSwap.sol";

/// @title Aggregator — atomic splitSwap over allowlisted venues with Permit2 pull.
/// @notice Non-upgradeable. Factory allowlist gates every hop; Permit2
///      AllowanceTransfer pulls exact amountIn; leftover catalog tokens are
///      swept to msg.sender and asserted 0 by the Foundry suite.
contract Aggregator is Ownable, Pausable, ReentrancyGuard {
    using SafeERC20 for IERC20;

    enum DexType {
        Xyk,
        Stable,
        Clmm
    }

    struct Hop {
        address pool;
        DexType dexType;
        address tokenIn;
        address tokenOut;
        uint24 fee; // CLMM fee tier; 0 otherwise
    }

    struct SubRoute {
        uint256 amountIn;
        Hop[] hops;
    }

    /// Permit2 AllowanceTransfer.PermitSingle + EIP-712 signature.
    /// signature.length == 0 means "allowance already set; skip permit()".
    struct Permit2Pull {
        IAllowanceTransfer.PermitSingle permitSingle;
        bytes signature;
    }

    event Swap(
        address indexed sender,
        address indexed tokenIn,
        address indexed tokenOut,
        uint256 amountIn,
        uint256 amountOut,
        bool isSplit
    );

    error Expired();
    error ZeroAddress();
    error SameToken();
    error ZeroAmount();
    error InvalidRoutes();
    error PoolNotFromFactory();
    error PermitSpenderMismatch();
    error SlippageExceeded(uint256 got, uint256 min);
    error CallbackSenderMismatch();
    error CallbackPoolNotAllowlisted();
    error DirectEth();

    uint256 internal constant V2_FEE_NUM = 997;
    uint256 internal constant V2_FEE_DEN = 1000;
    // Uniswap V3 sqrt-price bounds (same constants as the V3 core).
    uint160 internal constant MIN_SQRT_RATIO = 4295128739;
    uint160 internal constant MAX_SQRT_RATIO = 1461446703485210103287273052203988822378723970341;

    address public immutable permit2;
    address public immutable usdc;
    address public immutable eurc;
    address public immutable mbtc;

    mapping(address => bool) public isFactory;
    mapping(address => DexType) public factoryDexType;
    address[] internal factoryList;
    mapping(address => uint256) internal factoryIndex;
    mapping(address => bool) public allowedStablePools;

    constructor(address _permit2, address _usdc, address _eurc, address _mbtc) Ownable(msg.sender) {
        permit2 = _permit2;
        usdc = _usdc;
        eurc = _eurc;
        mbtc = _mbtc;
    }

    // ─── Admin ────────────────────────────────────────────────

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    /// @notice Allowlist a venue factory with its DexType.
    function addFactory(address factory, DexType dexType) external onlyOwner {
        if (factory == address(0)) revert ZeroAddress();
        if (!isFactory[factory]) {
            isFactory[factory] = true;
            factoryIndex[factory] = factoryList.length;
            factoryList.push(factory);
        }
        factoryDexType[factory] = dexType;
    }

    /// @notice Remove a venue factory from the allowlist. Gated hops must fail after this.
    function removeFactory(address factory) external onlyOwner {
        if (!isFactory[factory]) return;
        uint256 index = factoryIndex[factory];
        uint256 last = factoryList.length - 1;
        if (index != last) {
            address moved = factoryList[last];
            factoryList[index] = moved;
            factoryIndex[moved] = index;
        }
        factoryList.pop();
        delete isFactory[factory];
        delete factoryIndex[factory];
        delete factoryDexType[factory];
    }

    /// @notice Owner-side pool allowlist for stable venues (no Uniswap-style getPair).
    function addStablePool(address pool) external onlyOwner {
        allowedStablePools[pool] = true;
    }

    function removeStablePool(address pool) external onlyOwner {
        delete allowedStablePools[pool];
    }

    /// @notice Testnet recovery of forced/stuck ERC-20. Not a fee skim; never
    ///         called on the swap path.
    function rescueTokens(address token, address to, uint256 amount) external onlyOwner {
        if (to == address(0)) revert ZeroAddress();
        IERC20(token).safeTransfer(to, amount);
    }

    // ─── ETH rejection ────────────────────────────────────────

    receive() external payable {
        revert DirectEth();
    }

    fallback() external payable {
        revert DirectEth();
    }

    // ─── Swap ─────────────────────────────────────────────────

    function splitSwap(
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 minAmountOut,
        uint256 deadline,
        SubRoute[] calldata routes,
        Permit2Pull calldata permit
    ) external nonReentrant whenNotPaused returns (uint256 amountOut) {
        _validate(deadline, tokenIn, tokenOut, amountIn, routes);
        _verifyPools(routes);
        _pull(msg.sender, tokenIn, amountIn, permit);

        uint256 tokenOutBefore = IERC20(tokenOut).balanceOf(address(this));
        for (uint256 r = 0; r < routes.length; r++) {
            SubRoute calldata sub = routes[r];
            uint256 hopIn = sub.amountIn;
            Hop[] calldata hops = sub.hops;
            for (uint256 h = 0; h < hops.length; h++) {
                hopIn = _executeHop(hops[h], hopIn);
            }
        }

        amountOut = IERC20(tokenOut).balanceOf(address(this)) - tokenOutBefore;
        if (amountOut < minAmountOut) revert SlippageExceeded(amountOut, minAmountOut);

        IERC20(tokenOut).safeTransfer(msg.sender, amountOut);
        _sweepCatalogTo(msg.sender);
        emit Swap(msg.sender, tokenIn, tokenOut, amountIn, amountOut, routes.length > 1);
    }

    // ─── Internal ─────────────────────────────────────────────

    function _validate(
        uint256 deadline,
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        SubRoute[] calldata routes
    ) internal view {
        if (block.timestamp > deadline) revert Expired();
        if (tokenIn == address(0) || tokenOut == address(0)) revert ZeroAddress();
        if (tokenIn == tokenOut) revert SameToken();
        if (amountIn == 0) revert ZeroAmount();
        if (routes.length == 0) revert InvalidRoutes();

        uint256 sum;
        for (uint256 r = 0; r < routes.length; r++) {
            Hop[] calldata hops = routes[r].hops;
            if (hops.length == 0) revert InvalidRoutes();
            sum += routes[r].amountIn;
            if (hops[0].tokenIn != tokenIn) revert InvalidRoutes();
            for (uint256 h = 0; h < hops.length; h++) {
                if (hops[h].tokenIn == address(0) || hops[h].tokenOut == address(0)) {
                    revert ZeroAddress();
                }
                if (hops[h].tokenIn == hops[h].tokenOut) revert InvalidRoutes();
                if (h > 0 && hops[h].tokenIn != hops[h - 1].tokenOut) revert InvalidRoutes();
            }
            if (hops[hops.length - 1].tokenOut != tokenOut) revert InvalidRoutes();
        }
        if (sum != amountIn) revert InvalidRoutes();
    }

    /// @notice Pre-flight: every hop's pool must belong to an allowlisted factory.
    function _verifyPools(SubRoute[] calldata routes) internal view {
        for (uint256 r = 0; r < routes.length; r++) {
            Hop[] calldata hops = routes[r].hops;
            for (uint256 h = 0; h < hops.length; h++) {
                _assertPool(hops[h]);
            }
        }
    }

    function _assertPool(Hop calldata hop) internal view {
        if (hop.dexType == DexType.Xyk) {
            for (uint256 i = 0; i < factoryList.length; i++) {
                address factory = factoryList[i];
                if (factoryDexType[factory] != DexType.Xyk) continue;
                if (IUniswapV2Factory(factory).getPair(hop.tokenIn, hop.tokenOut) == hop.pool) {
                    return;
                }
            }
        } else if (hop.dexType == DexType.Stable) {
            if (allowedStablePools[hop.pool]) return;
            for (uint256 i = 0; i < factoryList.length; i++) {
                address factory = factoryList[i];
                if (factoryDexType[factory] != DexType.Stable) continue;
                if (IStableSwapFactory(factory).getPool(hop.tokenIn, hop.tokenOut) == hop.pool) {
                    return;
                }
            }
        } else {
            for (uint256 i = 0; i < factoryList.length; i++) {
                address factory = factoryList[i];
                if (factoryDexType[factory] != DexType.Clmm) continue;
                if (
                    IUniswapV3Factory(factory).getPool(hop.tokenIn, hop.tokenOut, hop.fee)
                        == hop.pool
                ) {
                    return;
                }
            }
        }
        revert PoolNotFromFactory();
    }

    /// @notice Permit2 AllowanceTransfer pull. Empty signature = skip permit().
    function _pull(address from, address tokenIn, uint256 amountIn, Permit2Pull calldata permit)
        internal
    {
        if (permit.signature.length > 0) {
            if (permit.permitSingle.spender != address(this)) revert PermitSpenderMismatch();
            IAllowanceTransfer(permit2).permit(from, permit.permitSingle, permit.signature);
        }
        IAllowanceTransfer(permit2).transferFrom(from, address(this), uint160(amountIn), tokenIn);
    }

    /// @notice Execute one allowlisted hop; returns the hop's output amount.
    function _executeHop(Hop calldata hop, uint256 amountIn) internal returns (uint256 amountOut) {
        if (hop.dexType == DexType.Xyk) {
            return _xykOut(hop, amountIn);
        }
        if (hop.dexType == DexType.Stable) {
            return _stableOut(hop, amountIn);
        }
        return _clmmOut(hop, amountIn);
    }

    /// @notice xy=k: pre-transfer tokenIn, compute 997/1000 output from reserves,
    ///         request exactly that output from swap with empty callback data.
    function _xykOut(Hop calldata hop, uint256 amountIn) internal returns (uint256 amountOut) {
        IUniswapV2Pair pair = IUniswapV2Pair(hop.pool);
        address token0 = pair.token0();
        (uint112 r0, uint112 r1,) = pair.getReserves();

        bool inIsToken0 = hop.tokenIn == token0;
        uint256 reserveIn = inIsToken0 ? uint256(r0) : uint256(r1);
        uint256 reserveOut = inIsToken0 ? uint256(r1) : uint256(r0);
        amountOut =
            (amountIn * V2_FEE_NUM * reserveOut) / (reserveIn * V2_FEE_DEN + amountIn * V2_FEE_NUM);

        IERC20(hop.tokenIn).safeTransfer(hop.pool, amountIn);
        pair.swap(inIsToken0 ? 0 : amountOut, inIsToken0 ? amountOut : 0, address(this), "");
    }

    /// @notice Stable: pre-transfer tokenIn, then exchange (pool has no transferFrom).
    function _stableOut(Hop calldata hop, uint256 amountIn) internal returns (uint256 dy) {
        IStableSwap pool = IStableSwap(hop.pool);
        uint8 i = hop.tokenIn == pool.token0() ? 0 : 1;
        uint8 j = i == 0 ? 1 : 0;
        IERC20(hop.tokenIn).safeTransfer(hop.pool, amountIn);
        dy = pool.exchange(i, j, amountIn, 0);
    }

    /// @notice CLMM: exact-in swap; callback pays the pool. Returns output from deltas.
    function _clmmOut(Hop calldata hop, uint256 amountIn) internal returns (uint256 amountOut) {
        IUniswapV3Pool pool = IUniswapV3Pool(hop.pool);
        bool zeroForOne = hop.tokenIn == pool.token0();
        uint160 sqrtLimit = zeroForOne ? MIN_SQRT_RATIO + 1 : MAX_SQRT_RATIO - 1;
        (int256 d0, int256 d1) = pool.swap(
            address(this),
            zeroForOne,
            int256(amountIn),
            sqrtLimit,
            abi.encode(hop.pool, hop.tokenIn)
        );
        amountOut = zeroForOne ? uint256(-d1) : uint256(-d0);
    }

    /// @notice V3 swap callback. Not nonReentrant: the pool re-enters while
    ///         splitSwap already holds the guard. Sender must be the allowlisted
    ///         pool encoded in `data`.
    function uniswapV3SwapCallback(int256 amount0Delta, int256 amount1Delta, bytes calldata data)
        external
    {
        (address pool, address tokenIn) = abi.decode(data, (address, address));
        if (msg.sender != pool) revert CallbackSenderMismatch();
        if (!_isAllowlistedClmmPool(pool)) revert CallbackPoolNotAllowlisted();
        uint256 amountToPay = amount0Delta > 0 ? uint256(amount0Delta) : uint256(amount1Delta);
        IERC20(tokenIn).safeTransfer(pool, amountToPay);
    }

    function _isAllowlistedClmmPool(address pool) internal view returns (bool) {
        IUniswapV3Pool p = IUniswapV3Pool(pool);
        address t0 = p.token0();
        address t1 = p.token1();
        uint24 fee = p.fee();
        for (uint256 i = 0; i < factoryList.length; i++) {
            address factory = factoryList[i];
            if (factoryDexType[factory] != DexType.Clmm) continue;
            if (IUniswapV3Factory(factory).getPool(t0, t1, fee) == pool) return true;
        }
        return false;
    }

    /// @notice Send all leftover catalog-token balances to `to` (nonzero-dust only).
    function _sweepCatalogTo(address to) internal {
        _sweepToken(usdc, to);
        _sweepToken(eurc, to);
        _sweepToken(mbtc, to);
    }

    function _sweepToken(address token, address to) internal {
        uint256 bal = IERC20(token).balanceOf(address(this));
        if (bal > 0) IERC20(token).safeTransfer(to, bal);
    }
}
