// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";
import {MockErc20} from "../src/MockErc20.sol";
import {MockBtc} from "../src/MockBtc.sol";
import {IUniswapV3Factory} from "../src/interfaces/IUniswapV3Factory.sol";
import {IUniswapV3Pool} from "../src/interfaces/IUniswapV3Pool.sol";

/// @notice T2.4 — Uniswap V3 CLMM 30 bps: createPool, initialize, mint, swap.
/// @dev V3 (solc 0.7.6) deployed via hex bytecodes.
///      Test contract implements V3 mint/swap callbacks in 0.8.30.
contract ClmmPoolTest is Test, VendorDeployer {
    IUniswapV3Factory internal factory;

    MockErc20 internal usdc;
    MockBtc internal mbtc;

    address internal alice = address(0xA11CE);

    uint160 internal constant MIN_SQRT_RATIO = 4295128739;
    uint160 internal constant MAX_SQRT_RATIO = 1461446703485210103287273052203988822378723970341;

    function uniswapV3MintCallback(uint256 amount0Owed, uint256 amount1Owed, bytes calldata)
        external
    {
        if (amount0Owed > 0) {
            MockErc20(IUniswapV3Pool(msg.sender).token0()).transfer(msg.sender, amount0Owed);
        }
        if (amount1Owed > 0) {
            MockErc20(IUniswapV3Pool(msg.sender).token1()).transfer(msg.sender, amount1Owed);
        }
    }

    function uniswapV3SwapCallback(int256 amount0Delta, int256 amount1Delta, bytes calldata)
        external
    {
        if (amount0Delta > 0) {
            MockErc20(IUniswapV3Pool(msg.sender).token0())
                .transfer(msg.sender, uint256(amount0Delta));
        }
        if (amount1Delta > 0) {
            MockErc20(IUniswapV3Pool(msg.sender).token1())
                .transfer(msg.sender, uint256(amount1Delta));
        }
    }

    function setUp() public {
        address factoryAddr = _deployFromHexFile("bytecodes/v3-factory.hex");
        factory = IUniswapV3Factory(factoryAddr);
        usdc = new MockErc20("USD Coin", "USDC");
        mbtc = new MockBtc();
    }

    /// @dev Price = 1e8/1e6 = 100 (1 USDC = 1 mBTC nominal) → sqrtPriceX96 = 10 * 2^96.
    function _sqrtPriceX96() internal pure returns (uint160) {
        return 792281625142643375935439503360; // 10 * 2^96
    }

    function _createPool() internal returns (IUniswapV3Pool) {
        address pool = factory.createPool(address(usdc), address(mbtc), 3000);
        IUniswapV3Pool p = IUniswapV3Pool(pool);
        p.initialize(_sqrtPriceX96());
        return p;
    }

    // ─── Factory basics ────────────────────────────────────────

    function test_createPool_and_slot0() public {
        IUniswapV3Pool p = _createPool();
        IUniswapV3Pool.Slot0 memory slot0 = p.slot0();
        assertEq(slot0.sqrtPriceX96, _sqrtPriceX96());
        assertTrue(slot0.unlocked, "pool locked");
    }

    // ─── Mint (30 bps, tickSpacing 60) ─────────────────────────

    function test_mint_inRange() public {
        IUniswapV3Pool p = _createPool();

        // Full range, L = 1e12: pays 1e11 USDC (0.1 USDC) + 1e13 mBTC
        usdc.mint(address(this), 100_000e6);
        mbtc.mint(address(this), 100_000e8);
        p.mint(alice, -887220, 887220, 1e12, "");

        assertTrue(p.liquidity() > 0, "no liquidity");
    }

    // ─── Swap ──────────────────────────────────────────────────

    function test_swap_zeroForOne() public {
        IUniswapV3Pool p = _createPool();

        usdc.mint(address(this), 100_000e6);
        mbtc.mint(address(this), 100_000e8);
        p.mint(alice, -887220, 887220, 1e12, "");

        // Tiny swap: 0.0001 USDC (100 units at 6dp)
        usdc.mint(address(this), 100);
        mbtc.mint(address(this), 100);

        uint256 mbtcBefore = mbtc.balanceOf(alice);
        (int256 amount0Delta, int256 amount1Delta) =
            p.swap(alice, true, 100, MIN_SQRT_RATIO + 1, "");
        assertTrue(amount0Delta > 0, "should pay USDC");
        assertTrue(amount1Delta < 0, "should receive mBTC");

        uint256 mbtcReceived = mbtc.balanceOf(alice) - mbtcBefore;
        assertTrue(mbtcReceived > 0, "no mBTC received");
    }

    function test_swap_oneForZero() public {
        IUniswapV3Pool p = _createPool();

        usdc.mint(address(this), 100_000e6);
        mbtc.mint(address(this), 100_000e8);
        p.mint(alice, -887220, 887220, 1e12, "");

        // Tiny swap: 1000 mBTC units (1e-5 mBTC) for ~9.97 USDC units
        usdc.mint(address(this), 100);
        mbtc.mint(address(this), 1000);

        uint256 usdcBefore = usdc.balanceOf(alice);
        (int256 amount0Delta, int256 amount1Delta) =
            p.swap(alice, false, 1000, MAX_SQRT_RATIO - 1, "");
        assertTrue(amount1Delta > 0, "should pay mBTC");
        assertTrue(amount0Delta < 0, "should receive USDC");

        uint256 usdcReceived = usdc.balanceOf(alice) - usdcBefore;
        assertTrue(usdcReceived > 0, "no USDC received");
    }

    // ─── Skip 5 bps pool ──────────────────────────────────────

    function test_no_5bps_pool() public view {
        assertEq(factory.getPool(address(usdc), address(mbtc), 500), address(0));
    }
}
