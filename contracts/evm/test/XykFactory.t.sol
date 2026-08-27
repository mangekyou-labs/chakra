// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";
import {MockErc20} from "../src/MockErc20.sol";
import {MockBtc} from "../src/MockBtc.sol";
import {IUniswapV2Factory} from "../src/interfaces/IUniswapV2Factory.sol";
import {IUniswapV2Pair} from "../src/interfaces/IUniswapV2Pair.sol";

/// @notice T2.2 — Uniswap V2 xy=k factory, pairs, mint/swap/burn.
/// @dev V2 (solc 0.5.16) deployed via hex bytecodes from `bytecodes/v2-*.hex`.
///      Tests talk through 0.8.30 interfaces only.
contract XykFactoryTest is Test, VendorDeployer {
    IUniswapV2Factory internal factory;

    MockErc20 internal usdc;
    MockErc20 internal eurc;
    MockBtc internal mbtc;

    address internal alice = address(0xA11CE);

    function setUp() public {
        address factoryAddr =
            _deployFromHexFileWithArgs("bytecodes/v2-factory.hex", abi.encode(address(0)));
        factory = IUniswapV2Factory(factoryAddr);

        usdc = new MockErc20("USD Coin", "USDC");
        eurc = new MockErc20("Euro Coin", "EURC");
        mbtc = new MockBtc();
    }

    // ─── Factory basics ────────────────────────────────────────

    function test_createPair_usdc_eurc() public {
        address pair = factory.createPair(address(usdc), address(eurc));
        assertTrue(pair != address(0), "pair address zero");
        assertEq(factory.getPair(address(usdc), address(eurc)), pair);
        assertEq(factory.getPair(address(eurc), address(usdc)), pair);
    }

    function test_createPair_usdc_mbtc() public {
        address pair = factory.createPair(address(usdc), address(mbtc));
        assertTrue(pair != address(0));
        assertEq(factory.getPair(address(usdc), address(mbtc)), pair);
    }

    function test_createPair_eurc_mbtc() public {
        address pair = factory.createPair(address(eurc), address(mbtc));
        assertTrue(pair != address(0));
        assertEq(factory.getPair(address(eurc), address(mbtc)), pair);
    }

    function test_token0_before_token1() public {
        address pairAddr = factory.createPair(address(usdc), address(eurc));
        IUniswapV2Pair pair = IUniswapV2Pair(pairAddr);
        assertTrue(pair.token0() < pair.token1(), "token0 must be < token1");
    }

    // ─── Mint ──────────────────────────────────────────────────

    function test_mint_reserves() public {
        address pairAddr = factory.createPair(address(usdc), address(eurc));
        IUniswapV2Pair pair = IUniswapV2Pair(pairAddr);

        uint256 amount0 = 10_000e6;
        uint256 amount1 = 10_000e6;
        _mintAndTransfer(address(usdc), pairAddr, amount0);
        _mintAndTransfer(address(eurc), pairAddr, amount1);
        pair.mint(alice);

        (uint112 r0, uint112 r1,) = pair.getReserves();
        assertTrue(r0 > 0 && r1 > 0, "reserves zero");
        assertEq(uint256(r0), amount0);
        assertEq(uint256(r1), amount1);
    }

    // ─── Swap ──────────────────────────────────────────────────

    function test_swap() public {
        address pairAddr = factory.createPair(address(usdc), address(eurc));
        IUniswapV2Pair pair = IUniswapV2Pair(pairAddr);

        uint256 liq0 = 50_000e6;
        uint256 liq1 = 50_000e6;
        _mintAndTransfer(address(usdc), pairAddr, liq0);
        _mintAndTransfer(address(eurc), pairAddr, liq1);
        pair.mint(alice);

        uint256 swapIn = 1_000e6;
        uint256 amountOut = (swapIn * 997 * liq1) / (liq0 * 1000 + swapIn * 997);

        address tokenIn;
        address tokenOut;
        if (pair.token0() == address(usdc)) {
            tokenIn = address(usdc);
            tokenOut = address(eurc);
        } else {
            tokenIn = address(eurc);
            tokenOut = address(usdc);
        }

        _mintAndTransfer(tokenIn, pairAddr, swapIn);

        uint256 balBefore = MockErc20(tokenOut).balanceOf(alice);
        if (pair.token0() == tokenIn) {
            pair.swap(0, amountOut, alice, "");
        } else {
            pair.swap(amountOut, 0, alice, "");
        }
        uint256 balAfter = MockErc20(tokenOut).balanceOf(alice);
        assertEq(balAfter - balBefore, amountOut, "swap output mismatch");
    }

    // ─── Burn ──────────────────────────────────────────────────

    function test_burn() public {
        address pairAddr = factory.createPair(address(usdc), address(eurc));
        IUniswapV2Pair pair = IUniswapV2Pair(pairAddr);

        uint256 liq0 = 10_000e6;
        uint256 liq1 = 10_000e6;
        _mintAndTransfer(address(usdc), pairAddr, liq0);
        _mintAndTransfer(address(eurc), pairAddr, liq1);
        pair.mint(alice);

        uint256 lpBal = pair.balanceOf(alice);
        assertTrue(lpBal > 0, "no LP tokens");

        vm.prank(alice);
        pair.transfer(pairAddr, lpBal);

        uint256 pairLp = pair.balanceOf(pairAddr);
        assertEq(pairLp, lpBal, "LP not at pair");

        pair.burn(alice);
        assertEq(pair.balanceOf(alice), 0, "LP not burned");
    }

    // ─── Fee invariant (30 bps) ────────────────────────────────

    function test_fee_is_30_bps() public {
        address pairAddr = factory.createPair(address(usdc), address(eurc));
        IUniswapV2Pair pair = IUniswapV2Pair(pairAddr);

        uint256 reserve = 100_000e6;
        _mintAndTransfer(address(usdc), pairAddr, reserve);
        _mintAndTransfer(address(eurc), pairAddr, reserve);
        pair.mint(alice);

        uint256 swapIn = 10_000e6;
        uint256 expectedOut = (swapIn * 997 * reserve) / (reserve * 1000 + swapIn * 997);
        uint256 noFeeOut = (swapIn * reserve) / (reserve + swapIn);
        assertTrue(expectedOut < noFeeOut, "fee has no effect");

        address tokenIn;
        address tokenOut;
        if (pair.token0() == address(usdc)) {
            tokenIn = address(usdc);
            tokenOut = address(eurc);
        } else {
            tokenIn = address(eurc);
            tokenOut = address(usdc);
        }

        _mintAndTransfer(tokenIn, pairAddr, swapIn);
        uint256 balBefore = MockErc20(tokenOut).balanceOf(alice);
        if (pair.token0() == tokenIn) {
            pair.swap(0, expectedOut, alice, "");
        } else {
            pair.swap(expectedOut, 0, alice, "");
        }
        uint256 balAfter = MockErc20(tokenOut).balanceOf(alice);
        assertEq(balAfter - balBefore, expectedOut, "fee output mismatch");
    }

    // ─── Helpers ───────────────────────────────────────────────

    function _mintAndTransfer(address token, address to, uint256 amount) internal {
        MockErc20(token).mint(address(this), amount);
        MockErc20(token).transfer(to, amount);
    }
}
