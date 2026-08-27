// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";
import {MockErc20} from "../src/MockErc20.sol";
import {IUniswapV2Factory} from "../src/interfaces/IUniswapV2Factory.sol";
import {IUniswapV2Pair} from "../src/interfaces/IUniswapV2Pair.sol";
import {StableSwapFactory} from "../src/stable/StableSwapFactory.sol";
import {StableSwap} from "../src/stable/StableSwap.sol";

/// @notice T2.3 — StableSwap USDC/EURC A=100, 4 bps, depth ≥20× xy=k.
contract StableSwapTest is Test, VendorDeployer {
    StableSwapFactory internal stableFactory;
    IUniswapV2Factory internal xykFactory;

    MockErc20 internal usdc;
    MockErc20 internal eurc;

    function setUp() public {
        stableFactory = new StableSwapFactory();

        address xykAddr =
            _deployFromHexFileWithArgs("bytecodes/v2-factory.hex", abi.encode(address(0)));
        xykFactory = IUniswapV2Factory(xykAddr);

        usdc = new MockErc20("USD Coin", "USDC");
        eurc = new MockErc20("Euro Coin", "EURC");
    }

    // ─── Factory basics ────────────────────────────────────────

    function test_createPool() public {
        address pool = stableFactory.createPool(address(usdc), address(eurc));
        assertTrue(pool != address(0));
        assertEq(stableFactory.getPool(address(usdc), address(eurc)), pool);
        assertEq(stableFactory.getPool(address(eurc), address(usdc)), pool);
    }

    function test_createPool_same_tokens_reverts() public {
        stableFactory.createPool(address(usdc), address(eurc));
        vm.expectRevert(StableSwapFactory.PoolExists.selector);
        stableFactory.createPool(address(usdc), address(eurc));
    }

    function test_createPool_reverse_same_pool() public {
        stableFactory.createPool(address(usdc), address(eurc));
        vm.expectRevert(StableSwapFactory.PoolExists.selector);
        stableFactory.createPool(address(eurc), address(usdc));
    }

    // ─── Exchange ──────────────────────────────────────────────

    function test_exchange0to1() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        uint256 swapIn = 1_000e6;
        address tokenInAddr = s.token0();
        address tokenOutAddr = s.token1();
        _mintAndTransfer(tokenInAddr, pool, swapIn);

        uint256 balBefore = MockErc20(tokenOutAddr).balanceOf(address(this));
        s.exchange(0, 1, swapIn, 0);
        uint256 dy = MockErc20(tokenOutAddr).balanceOf(address(this)) - balBefore;

        assertTrue(dy >= 999_000_000, "output too low");
        assertTrue(dy <= swapIn, "output exceeds input");
    }

    function test_exchange1to0() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        uint256 swapIn = 1_000e6;
        _mintAndTransfer(s.token1(), pool, swapIn);

        uint256 balBefore = MockErc20(s.token0()).balanceOf(address(this));
        s.exchange(1, 0, swapIn, 0);
        uint256 dy = MockErc20(s.token0()).balanceOf(address(this)) - balBefore;

        assertTrue(dy >= 999_000_000, "output too low");
        assertTrue(dy <= swapIn, "output exceeds input");
    }

    function test_minDy_respected() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);
        uint256 swapIn = 1_000e6;
        _mintAndTransfer(s.token0(), pool, swapIn);
        vm.expectRevert();
        s.exchange(0, 1, swapIn, swapIn);
    }

    function test_same_index_reverts() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);
        vm.expectRevert(StableSwap.SameIndex.selector);
        s.exchange(0, 0, 100e6, 0);
    }

    function test_zero_amount_reverts() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);
        vm.expectRevert(StableSwap.ZeroAmount.selector);
        s.exchange(0, 1, 0, 0);
    }

    // ─── Fee check ─────────────────────────────────────────────

    function test_fee_is_4_bps() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        uint256 swapIn = 10_000e6;
        _mintAndTransfer(s.token0(), pool, swapIn);

        uint256 balBefore = MockErc20(s.token1()).balanceOf(address(this));
        s.exchange(0, 1, swapIn, 0);
        uint256 dy = MockErc20(s.token1()).balanceOf(address(this)) - balBefore;

        assertTrue(dy >= 9_990e6, "fee too high");
        assertTrue(dy < swapIn, "no fee applied");
    }

    // ─── Depth vs xy=k ────────────────────────────────────────

    function test_stable_deeper_than_xyk() public {
        // xy=k: 10k each
        address xykPair = xykFactory.createPair(address(usdc), address(eurc));
        IUniswapV2Pair xyk = IUniswapV2Pair(xykPair);
        _mintAndTransfer(address(usdc), xykPair, 10_000e6);
        _mintAndTransfer(address(eurc), xykPair, 10_000e6);
        xyk.mint(address(this));

        // stable: 200k each
        address stablePool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(stablePool);

        uint256 swapIn = 1_000e6;

        // xy=k output (997/1000 fee)
        _mintAndTransfer(address(usdc), xykPair, swapIn);
        uint256 xykBefore = eurc.balanceOf(address(this));
        uint256 xykDy = (swapIn * 997 * 10_000e6) / (10_000e6 * 1000 + swapIn * 997);
        if (xyk.token0() == address(usdc)) {
            xyk.swap(0, xykDy, address(this), "");
        } else {
            xyk.swap(xykDy, 0, address(this), "");
        }
        uint256 xykOut = eurc.balanceOf(address(this)) - xykBefore;

        // stable: send whatever is token0, get whatever is token1
        _mintAndTransfer(s.token0(), stablePool, swapIn);
        address stableTokenOut = s.token1();
        uint256 stableBefore = MockErc20(stableTokenOut).balanceOf(address(this));
        s.exchange(0, 1, swapIn, 0);
        uint256 stableOut = MockErc20(stableTokenOut).balanceOf(address(this)) - stableBefore;

        assertTrue(stableOut > xykOut, "stable not deeper than xy=k");
    }

    // ─── Custody: stored reserves, deposit proof, index bounds ──

    function test_exchange_without_deposit_reverts() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        // No tokens transferred to pool — must revert (no deposit proof)
        vm.expectRevert(StableSwap.ZeroAmount.selector);
        s.exchange(0, 1, 100e6, 0);
    }

    function test_exchange_rejects_index_out_of_range() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        vm.expectRevert(StableSwap.IndexOutOfRange.selector);
        s.exchange(2, 1, 100e6, 0);

        vm.expectRevert(StableSwap.IndexOutOfRange.selector);
        s.exchange(0, 2, 100e6, 0);

        vm.expectRevert(StableSwap.IndexOutOfRange.selector);
        s.exchange(5, 5, 100e6, 0);
    }

    function test_exchange_reverts_when_declared_amount_exceeds_actual_deposit() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        // Transfer only 50e6 but declare 100e6
        _mintAndTransfer(s.token0(), pool, 50e6);
        vm.expectRevert(StableSwap.InsufficientInput.selector);
        s.exchange(0, 1, 100e6, 0);
    }

    function test_reserves_updated_after_exchange() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        assertEq(s.reserve0(), 200_000e6);
        assertEq(s.reserve1(), 200_000e6);

        uint256 swapIn = 1_000e6;
        _mintAndTransfer(s.token0(), pool, swapIn);
        s.exchange(0, 1, swapIn, 0);

        // reserve0 increased by swapIn, reserve1 decreased by dy
        assertTrue(s.reserve0() > 200_000e6, "reserve0 not increased");
        assertTrue(s.reserve1() < 200_000e6, "reserve1 not decreased");
    }

    function test_reserves_updated_after_remove_liquidity() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        uint256 lpBal = s.balanceOf(address(this));
        uint256 halfLp = lpBal / 2;

        uint256 r0Before = s.reserve0();
        uint256 r1Before = s.reserve1();

        s.removeLiquidity(halfLp);

        assertEq(s.reserve0(), r0Before / 2, "reserve0 should halve");
        assertEq(s.reserve1(), r1Before / 2, "reserve1 should halve");
    }

    function test_exchange_excess_deposit_not_consumed() public {
        address pool = _createAndSeedPool(200_000e6, 200_000e6);
        StableSwap s = StableSwap(pool);

        // Transfer 2000e6 but only declare 1000e6 — should succeed
        uint256 swapIn = 1_000e6;
        _mintAndTransfer(s.token0(), pool, 2000e6);
        s.exchange(0, 1, swapIn, 0);

        // reserve0 should be 200_000 + 1000 (only declared amount, not excess)
        assertEq(s.reserve0(), 200_000e6 + swapIn, "reserve0 consumed full deposit");
    }

    // ─── Helpers ───────────────────────────────────────────────

    function _createAndSeedPool(uint256 amount0, uint256 amount1) internal returns (address pool) {
        pool = stableFactory.createPool(address(usdc), address(eurc));
        StableSwap s = StableSwap(pool);
        _mintAndTransfer(s.token0(), pool, amount0);
        _mintAndTransfer(s.token1(), pool, amount1);
        StableSwap(pool).seedLiquidity(amount0, amount1);
    }

    function _mintAndTransfer(address token, address to, uint256 amount) internal {
        MockErc20(token).mint(address(this), amount);
        MockErc20(token).transfer(to, amount);
    }
}
