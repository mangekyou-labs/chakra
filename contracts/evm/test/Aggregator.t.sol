// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {MockErc20} from "../src/MockErc20.sol";
import {MockBtc} from "../src/MockBtc.sol";
import {MockPermit2} from "./MockPermit2.sol";
import {Aggregator} from "../src/Aggregator.sol";
import {IUniswapV2Factory} from "../src/interfaces/IUniswapV2Factory.sol";
import {IUniswapV2Pair} from "../src/interfaces/IUniswapV2Pair.sol";
import {IUniswapV3Factory} from "../src/interfaces/IUniswapV3Factory.sol";
import {IUniswapV3Pool} from "../src/interfaces/IUniswapV3Pool.sol";
import {StableSwapFactory} from "../src/stable/StableSwapFactory.sol";
import {StableSwap} from "../src/stable/StableSwap.sol";
import {MockXyloFactory, MockXyloPool} from "../src/MockXylo.sol";
import {IAllowanceTransfer} from "../src/interfaces/IAllowanceTransfer.sol";

/// @notice T5.1 — Aggregator splitSwap: pause, validation, ETH, factory allowlist,
///      Permit2, xyk/stable/clmm hops, leftover-0, rescue, never-call.
contract AggregatorTest is Test, VendorDeployer {
    Aggregator internal agg;
    MockPermit2 internal permit2;
    MockErc20 internal usdc;
    MockErc20 internal eurc;
    MockBtc internal mbtc;

    // Deployer == owner (test contract).
    address internal user = address(0xBEEF);
    address internal alice = address(0xA11CE);

    IUniswapV2Factory internal xykFactory;
    StableSwapFactory internal stableFactory;
    IUniswapV3Factory internal clmmFactory;
    MockXyloFactory internal xyloFactory;

    uint256 internal constant XYK_SEED = 10_000e6;
    uint256 internal constant STABLE_SEED = 200_000e6;

    /// @dev Arc contract-addresses.md never-call table (CCTP V2 / Gateway / USYC /
    ///      FxEscrow / Memo / Multicall3From). Never allowlisted, never routed.
    address[12] internal neverCall;

    function setUp() public {
        neverCall = [
            address(0x8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA), // CCTP TokenMessengerV2
            address(0xE737e5cEBEEBa77EFE34D4aa090756590b1CE275), // CCTP MessageTransmitterV2
            address(0xb43db544E2c27092c107639Ad201b3dEfAbcF192), // CCTP TokenMinterV2
            address(0xbaC0179bB358A8936169a63408C8481D582390C4), // CCTP MessageV2
            address(0x0077777d7EBA4688BDeF3E311b846F25870A19B9), // GatewayWallet
            address(0x0022222ABE238Cc2C7Bb1f21003F0a260052475B), // GatewayMinter
            address(0x867650F5eAe8df91445971f14d89fd84F0C9a9f8), // StableFX FxEscrow
            address(0xe9185F0c5F296Ed1797AaE4238D26CCaBEadb86C), // USYC
            address(0xCC205224862C7641930c87679E98999d23C26113), // USYC Entitlements
            address(0x9fdF14c5B14173D74C08Af27AebFf39240dC105A), // USYC Teller
            address(0x9702466268ccF55eAB64cdf484d272Ac08d3b75b), // Memo
            address(0xEb7cc06E3D3b5F9F9a5fA2B31B477ff72bB9c8b6) // Multicall3From
        ];
        permit2 = new MockPermit2();
        usdc = new MockErc20("USD Coin", "USDC");
        eurc = new MockErc20("Euro Coin", "EURC");
        mbtc = new MockBtc();

        agg = new Aggregator(address(permit2), address(usdc), address(eurc), address(mbtc));

        // Venue factories: xyk = V2 hex, stable = original, clmm = V3 hex.
        address xykAddr =
            _deployFromHexFileWithArgs("bytecodes/v2-factory.hex", abi.encode(address(0)));
        xykFactory = IUniswapV2Factory(xykAddr);
        stableFactory = new StableSwapFactory();
        address clmmAddr = _deployFromHexFile("bytecodes/v3-factory.hex");
        clmmFactory = IUniswapV3Factory(clmmAddr);
        xyloFactory = new MockXyloFactory();
    }

    /// @dev V3 pool calls back the minter with the owed amounts (T2.4 fixture).
    function uniswapV3MintCallback(uint256 amount0Owed, uint256 amount1Owed, bytes calldata)
        external
    {
        IUniswapV3Pool pool = IUniswapV3Pool(msg.sender);
        if (amount0Owed > 0) {
            if (pool.token0() == address(usdc)) usdc.transfer(msg.sender, amount0Owed);
            else mbtc.transfer(msg.sender, amount0Owed);
        }
        if (amount1Owed > 0) {
            if (pool.token1() == address(usdc)) usdc.transfer(msg.sender, amount1Owed);
            else mbtc.transfer(msg.sender, amount1Owed);
        }
    }

    // ─── 1. Pausable, owner-only ──────────────────────────────

    function test_nonOwner_cannot_pause() public {
        vm.prank(user);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, user));
        agg.pause();
    }

    function test_nonOwner_cannot_unpause() public {
        agg.pause();
        vm.prank(user);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, user));
        agg.unpause();
    }

    function test_owner_pause_blocks_splitSwap() public {
        agg.pause();
        assertTrue(agg.paused());
        vm.expectRevert(Pausable.EnforcedPause.selector);
        _callSplitSwap(address(usdc), address(eurc), 100e6, 0, block.timestamp);
    }

    function test_owner_unpause_restores() public {
        agg.pause();
        assertTrue(agg.paused());
        agg.unpause();
        assertFalse(agg.paused());
    }

    // ─── 2. Input validation ──────────────────────────────────

    function test_deadline_past_reverts() public {
        vm.expectRevert(Aggregator.Expired.selector);
        _callSplitSwap(address(usdc), address(eurc), 100e6, 0, block.timestamp - 1);
    }

    function test_tokenIn_equals_tokenOut_reverts() public {
        vm.expectRevert(Aggregator.SameToken.selector);
        _callSplitSwap(address(usdc), address(usdc), 100e6, 0, block.timestamp);
    }

    function test_zero_token_address_reverts() public {
        vm.expectRevert(Aggregator.ZeroAddress.selector);
        _callSplitSwap(address(0), address(eurc), 100e6, 0, block.timestamp);
    }

    function test_zero_amount_reverts() public {
        vm.expectRevert(Aggregator.ZeroAmount.selector);
        _callSplitSwap(address(usdc), address(eurc), 0, 0, block.timestamp);
    }

    function test_empty_routes_reverts() public {
        vm.expectRevert(Aggregator.InvalidRoutes.selector);
        _callSplitSwap(address(usdc), address(eurc), 100e6, 0, block.timestamp);
    }

    function test_amount_sum_mismatch_reverts() public {
        Aggregator.Hop memory h =
            _hop(address(0), Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 60e6);
        vm.expectRevert(Aggregator.InvalidRoutes.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    function test_first_hop_token_mismatch_reverts() public {
        Aggregator.Hop memory h =
            _hop(address(0), Aggregator.DexType.Xyk, address(mbtc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
        vm.expectRevert(Aggregator.InvalidRoutes.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    function test_last_hop_token_mismatch_reverts() public {
        Aggregator.Hop memory h =
            _hop(address(0), Aggregator.DexType.Xyk, address(usdc), address(mbtc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
        vm.expectRevert(Aggregator.InvalidRoutes.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    function test_hop_continuity_reverts() public {
        Aggregator.SubRoute[] memory routes = new Aggregator.SubRoute[](1);
        routes[0].amountIn = 100e6;
        routes[0].hops = new Aggregator.Hop[](2);
        routes[0].hops[0] = _hop(address(0), Aggregator.DexType.Xyk, address(usdc), address(eurc));
        routes[0].hops[1] = _hop(address(0), Aggregator.DexType.Xyk, address(usdc), address(mbtc));
        vm.expectRevert(Aggregator.InvalidRoutes.selector);
        agg.splitSwap(
            address(usdc), address(mbtc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    function test_empty_hops_reverts() public {
        Aggregator.SubRoute[] memory routes = new Aggregator.SubRoute[](1);
        routes[0].amountIn = 100e6;
        routes[0].hops = new Aggregator.Hop[](0);
        vm.expectRevert(Aggregator.InvalidRoutes.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    // ─── 3. ETH rejection ─────────────────────────────────────

    function test_receive_eth_reverts() public {
        (bool ok, bytes memory ret) = address(agg).call{value: 1}("");
        assertFalse(ok);
        assertEq(bytes4(ret), Aggregator.DirectEth.selector);
    }

    function test_fallback_eth_reverts() public {
        (bool ok, bytes memory ret) = address(agg).call{value: 1}(abi.encodeWithSignature("nope()"));
        assertFalse(ok);
        assertEq(bytes4(ret), Aggregator.DirectEth.selector);
    }

    function test_splitSwap_rejects_value() public {
        vm.deal(user, 1 ether);
        bytes memory data = abi.encodeWithSelector(
            agg.splitSwap.selector,
            address(usdc),
            address(eurc),
            100e6,
            0,
            block.timestamp,
            new Aggregator.SubRoute[](0),
            _emptyPermit()
        );
        vm.prank(user);
        (bool ok,) = address(agg).call{value: 1}(data);
        assertFalse(ok);
    }

    // ─── 4. Factory allowlist ─────────────────────────────────

    function test_addFactory_onlyOwner() public {
        vm.prank(user);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, user));
        agg.addFactory(address(xykFactory), Aggregator.DexType.Xyk);
    }

    function test_hop_to_fake_pool_reverts() public {
        address realPair = _seedXykPair(address(usdc), address(eurc), XYK_SEED, XYK_SEED);
        agg.addFactory(address(xykFactory), Aggregator.DexType.Xyk);
        assertTrue(realPair != address(0), "pair not created");

        Aggregator.Hop memory h =
            _hop(address(0xCAFE), Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
        vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    function test_hop_without_allowlisted_factory_reverts() public {
        address realPair = _seedXykPair(address(usdc), address(eurc), XYK_SEED, XYK_SEED);

        Aggregator.Hop memory h =
            _hop(realPair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
        vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    function test_removeFactory_gates_hops() public {
        address realPair = _seedXykPair(address(usdc), address(eurc), XYK_SEED, XYK_SEED);
        agg.addFactory(address(xykFactory), Aggregator.DexType.Xyk);
        agg.removeFactory(address(xykFactory));

        Aggregator.Hop memory h =
            _hop(realPair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
        vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    // ─── 5. Permit2 pull ───────────────────────────────────────

    function test_permit2_bad_signature_reverts() public {
        (address pair,) = _prepareXykSwap(1000e6);

        Aggregator.Hop memory h = _hop(pair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);
        Aggregator.Permit2Pull memory pull = _signedPull(1000e6, new bytes(64)); // malformed

        vm.prank(user);
        vm.expectRevert(MockPermit2.InvalidSignature.selector);
        agg.splitSwap(address(usdc), address(eurc), 1000e6, 0, block.timestamp, routes, pull);
    }

    function test_permit2_empty_signature_skips_permit() public {
        (address pair, uint256 expectedOut) = _prepareXykSwap(1000e6);
        _grantMockAllowance(address(usdc), 1000e6);

        Aggregator.Hop memory h = _hop(pair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);

        vm.prank(user);
        uint256 amountOut = agg.splitSwap(
            address(usdc),
            address(eurc),
            1000e6,
            expectedOut,
            block.timestamp,
            routes,
            _emptyPermit()
        );

        assertEq(permit2.permitCalls(), 0, "permit called for empty signature");
        assertEq(amountOut, expectedOut + 0, "output mismatch");
    }

    function test_permit2_signature_grants_allowance() public {
        (address pair, uint256 expectedOut) = _prepareXykSwap(1000e6);
        // No mock pre-grant — the signed permit must create the allowance.

        Aggregator.Hop memory h = _hop(pair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);
        Aggregator.Permit2Pull memory pull = _signedPull(1000e6, new bytes(65));

        vm.prank(user);
        uint256 amountOut = agg.splitSwap(
            address(usdc), address(eurc), 1000e6, expectedOut, block.timestamp, routes, pull
        );

        assertEq(permit2.permitCalls(), 1, "permit not called for signed pull");
        assertEq(amountOut, expectedOut, "output mismatch");
    }

    function test_permit2_spender_mismatch_reverts() public {
        (address pair,) = _prepareXykSwap(1000e6);

        Aggregator.Hop memory h = _hop(pair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);
        Aggregator.Permit2Pull memory pull = _signedPull(1000e6, new bytes(65));
        pull.permitSingle.spender = address(0x1234);

        vm.prank(user);
        vm.expectRevert(Aggregator.PermitSpenderMismatch.selector);
        agg.splitSwap(address(usdc), address(eurc), 1000e6, 0, block.timestamp, routes, pull);
    }

    // ─── 5b. ABI round-trip: API-style hex feed ────────────────

    /// @dev Feed the exact calldata layout the Rust encoder produces:
    ///      selector 0x2e3be0c1, Permit2Pull = 6-word zeroed PermitSingle
    ///      + offset(224) + empty sig. Empty signature path (permit skipped).
    function test_api_hex_empty_sig_succeeds() public {
        (address pair, uint256 expectedOut) = _prepareXykSwap(1000e6);
        _grantMockAllowance(address(usdc), 1000e6);

        Aggregator.Hop memory h = _hop(pair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);
        bytes memory correctData = abi.encodeWithSelector(
            0x2e3be0c1,
            address(usdc),
            address(eurc),
            uint256(1000e6),
            uint256(expectedOut),
            block.timestamp,
            routes,
            Aggregator.Permit2Pull({
                permitSingle: IAllowanceTransfer.PermitSingle({
                    details: IAllowanceTransfer.PermitDetails({
                        token: address(0), amount: 0, expiration: 0, nonce: 0
                    }),
                    spender: address(0),
                    sigDeadline: 0
                }),
                signature: ""
            })
        );

        // Verify selector bytes
        bytes4 sel;
        assembly { sel := mload(add(correctData, 32)) }
        assertEq(sel, bytes4(0x2e3be0c1), "selector must be 2e3be0c1");

        vm.prank(user);
        (bool ok, bytes memory ret) = address(agg).call(correctData);
        assertTrue(ok, "API hex call failed");
        uint256 amountOut = abi.decode(ret, (uint256));
        assertEq(amountOut, expectedOut, "output mismatch");
    }

    // ─── 6. Single-hop xy=k happy path ─────────────────────────

    function test_single_hop_xyk_success() public {
        (address pair, uint256 expectedOut) = _prepareXykSwap(1000e6);
        _grantMockAllowance(address(usdc), 1000e6);

        Aggregator.Hop memory h = _hop(pair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);

        vm.expectEmit(true, true, true, true, address(agg));
        emit Aggregator.Swap(user, address(usdc), address(eurc), 1000e6, expectedOut, false);

        vm.prank(user);
        uint256 amountOut = agg.splitSwap(
            address(usdc),
            address(eurc),
            1000e6,
            expectedOut,
            block.timestamp,
            routes,
            _emptyPermit()
        );

        assertEq(amountOut, expectedOut, "venue-only math violated");
        assertEq(eurc.balanceOf(user), expectedOut, "user did not receive output");
        assertEq(usdc.balanceOf(user), 0, "user USDC not fully spent");
        _assertCatalogZero();
    }

    // ─── 7. minAmountOut too high ──────────────────────────────

    function test_minAmountOut_too_high_reverts() public {
        (address pair, uint256 expectedOut) = _prepareXykSwap(1000e6);
        _grantMockAllowance(address(usdc), 1000e6);

        Aggregator.Hop memory h = _hop(pair, Aggregator.DexType.Xyk, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);

        (uint112 r0Before, uint112 r1Before,) = IUniswapV2Pair(pair).getReserves();
        uint256 userUsdcBefore = usdc.balanceOf(user);
        uint256 userEurcBefore = eurc.balanceOf(user);

        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(
                Aggregator.SlippageExceeded.selector, expectedOut, expectedOut + 1
            )
        );
        agg.splitSwap(
            address(usdc),
            address(eurc),
            1000e6,
            expectedOut + 1,
            block.timestamp,
            routes,
            _emptyPermit()
        );

        _assertCatalogZero();
        assertEq(usdc.balanceOf(user), userUsdcBefore, "user USDC moved on revert");
        assertEq(eurc.balanceOf(user), userEurcBefore, "user EURC changed on revert");
        (uint112 r0After, uint112 r1After,) = IUniswapV2Pair(pair).getReserves();
        assertEq(uint256(r0After), uint256(r0Before), "reserve0 changed on revert");
        assertEq(uint256(r1After), uint256(r1Before), "reserve1 changed on revert");
    }

    // ─── 8. Multi-hop (EURC via USDC), atomic ───────────────────

    function test_multi_hop_eurc_via_usdc_success() public {
        (address pairEurcUsdc, address pairUsdcMbtc, uint256 expectedOut) = _prepareMultiHop();

        Aggregator.SubRoute[] memory routes = new Aggregator.SubRoute[](1);
        routes[0].amountIn = 1000e6;
        routes[0].hops = new Aggregator.Hop[](2);
        routes[0].hops[0] = _hop(pairEurcUsdc, Aggregator.DexType.Xyk, address(eurc), address(usdc));
        routes[0].hops[1] = _hop(pairUsdcMbtc, Aggregator.DexType.Xyk, address(usdc), address(mbtc));

        vm.expectEmit(true, true, true, true, address(agg));
        emit Aggregator.Swap(user, address(eurc), address(mbtc), 1000e6, expectedOut, false);

        vm.prank(user);
        uint256 amountOut = agg.splitSwap(
            address(eurc),
            address(mbtc),
            1000e6,
            expectedOut,
            block.timestamp,
            routes,
            _emptyPermit()
        );

        assertEq(amountOut, expectedOut, "multi-hop output mismatch");
        assertEq(mbtc.balanceOf(user), expectedOut, "user mBTC mismatch");
        _assertCatalogZero();
    }

    function test_multi_hop_min_revert_is_atomic() public {
        (address pairEurcUsdc, address pairUsdcMbtc, uint256 expectedOut) = _prepareMultiHop();

        Aggregator.SubRoute[] memory routes = new Aggregator.SubRoute[](1);
        routes[0].amountIn = 1000e6;
        routes[0].hops = new Aggregator.Hop[](2);
        routes[0].hops[0] = _hop(pairEurcUsdc, Aggregator.DexType.Xyk, address(eurc), address(usdc));
        routes[0].hops[1] = _hop(pairUsdcMbtc, Aggregator.DexType.Xyk, address(usdc), address(mbtc));

        (uint112 a0, uint112 a1,) = IUniswapV2Pair(pairEurcUsdc).getReserves();
        (uint112 b0, uint112 b1,) = IUniswapV2Pair(pairUsdcMbtc).getReserves();
        uint256 userEurcBefore = eurc.balanceOf(user);
        uint256 userMbtcBefore = mbtc.balanceOf(user);

        vm.prank(user);
        vm.expectRevert(
            abi.encodeWithSelector(
                Aggregator.SlippageExceeded.selector, expectedOut, expectedOut + 1
            )
        );
        agg.splitSwap(
            address(eurc),
            address(mbtc),
            1000e6,
            expectedOut + 1,
            block.timestamp,
            routes,
            _emptyPermit()
        );

        _assertCatalogZero();
        assertEq(eurc.balanceOf(user), userEurcBefore, "user EURC moved on revert");
        assertEq(mbtc.balanceOf(user), userMbtcBefore, "user mBTC changed on revert");
        (uint112 a2, uint112 a3,) = IUniswapV2Pair(pairEurcUsdc).getReserves();
        (uint112 b2, uint112 b3,) = IUniswapV2Pair(pairUsdcMbtc).getReserves();
        assertEq(uint256(a2), uint256(a0), "pair1 reserve0 changed");
        assertEq(uint256(a3), uint256(a1), "pair1 reserve1 changed");
        assertEq(uint256(b2), uint256(b0), "pair2 reserve0 changed");
        assertEq(uint256(b3), uint256(b1), "pair2 reserve1 changed");
    }

    // ─── 9. Split: thin xy=k + deep stable ─────────────────────

    function test_split_thin_xyk_plus_deep_stable() public {
        (address xykPair, address stablePool) = _prepareSplit();

        (uint112 xr0, uint112 xr1,) = IUniswapV2Pair(xykPair).getReserves();
        StableSwap s = StableSwap(stablePool);
        uint256 sb0 = s.balance0();
        uint256 sb1 = s.balance1();

        Aggregator.SubRoute[] memory routes = new Aggregator.SubRoute[](2);
        routes[0].amountIn = 700e6;
        routes[0].hops = new Aggregator.Hop[](1);
        routes[0].hops[0] =
            _hop(stablePool, Aggregator.DexType.Stable, address(usdc), address(eurc));
        routes[1].amountIn = 300e6;
        routes[1].hops = new Aggregator.Hop[](1);
        routes[1].hops[0] = _hop(xykPair, Aggregator.DexType.Xyk, address(usdc), address(eurc));

        // Venue-fee-only bound: xyk formula + ≥699e6 stable leg (700e6 at ≤4 bps on 200k depth).
        uint256 expectedXykLeg = _xykFormula(IUniswapV2Pair(xykPair), address(usdc), 300e6);
        uint256 minBound = expectedXykLeg + 699e6;

        // Topics checked; data (amountOut) left unchecked — stable output is not closed-form.
        vm.expectEmit(true, true, true, false, address(agg));
        emit Aggregator.Swap(user, address(usdc), address(eurc), 1000e6, 0, true);

        vm.prank(user);
        uint256 amountOut = agg.splitSwap(
            address(usdc), address(eurc), 1000e6, minBound, block.timestamp, routes, _emptyPermit()
        );

        assertTrue(amountOut >= minBound, "protocol fee or low venue output");
        assertEq(eurc.balanceOf(user), amountOut, "user EURC mismatch");
        _assertCatalogZero();

        // Both venues' reserves must have moved (SC-4 unit analog).
        (uint112 xr2, uint112 xr3,) = IUniswapV2Pair(xykPair).getReserves();
        assertTrue(
            uint256(xr2) != uint256(xr0) || uint256(xr3) != uint256(xr1), "xyk reserves static"
        );
        assertTrue(s.balance0() != sb0 || s.balance1() != sb1, "stable reserves static");
    }

    // ─── 10. CLMM hop + callback spoof ─────────────────────────

    function test_clmm_hop_succeeds_via_callback() public {
        IUniswapV3Pool pool = _prepareClmm();

        Aggregator.Hop memory h =
            _hop(address(pool), Aggregator.DexType.Clmm, address(usdc), address(mbtc));
        h.fee = 3000;
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);

        vm.expectEmit(true, true, true, false, address(agg));
        emit Aggregator.Swap(user, address(usdc), address(mbtc), 100e6, 0, false);

        vm.prank(user);
        uint256 amountOut = agg.splitSwap(
            address(usdc), address(mbtc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );

        assertTrue(amountOut > 0, "no mBTC received");
        assertEq(mbtc.balanceOf(user), amountOut, "user mBTC mismatch");
        _assertCatalogZero();
    }

    function test_clmm_callback_sender_mismatch_reverts() public {
        IUniswapV3Pool pool = _prepareClmm();
        vm.prank(alice);
        vm.expectRevert(Aggregator.CallbackSenderMismatch.selector);
        agg.uniswapV3SwapCallback(1e6, 0, abi.encode(address(pool), address(usdc)));
    }

    function test_clmm_callback_non_allowlisted_pool_reverts() public {
        IUniswapV3Pool pool = _prepareClmm();
        FakePool fake = new FakePool(address(usdc), address(mbtc), 3000, agg);
        // fake pools as msg.sender and decoded pool — must fail the allowlist check.
        vm.expectRevert(Aggregator.CallbackPoolNotAllowlisted.selector);
        fake.spoof();
        assertTrue(address(pool) != address(fake), "fixture broken");
    }

    function test_clmm_callback_random_eoa_reverts() public {
        _prepareClmm();
        vm.expectRevert();
        vm.prank(address(0xFEED));
        agg.uniswapV3SwapCallback(1e6, 0, abi.encode(address(0xFEED), address(usdc)));
    }

    // ─── 11. rescueTokens ──────────────────────────────────────

    function test_rescueTokens_owner() public {
        usdc.mint(address(agg), 123e6); // forced/stuck ERC-20 on the aggregator
        agg.rescueTokens(address(usdc), alice, 123e6);
        assertEq(usdc.balanceOf(alice), 123e6, "rescued amount mismatch");
        assertEq(usdc.balanceOf(address(agg)), 0, "rescue leftover");
    }

    function test_rescueTokens_non_owner_reverts() public {
        usdc.mint(address(agg), 123e6);
        vm.prank(user);
        vm.expectRevert(abi.encodeWithSelector(Ownable.OwnableUnauthorizedAccount.selector, user));
        agg.rescueTokens(address(usdc), alice, 100e6);
    }

    // ─── 10b. Xylo hop (T-XYLO) ────────────────────────────────

    /// @dev Seed a catalog USDC/EURC Xylo pool (deep, off-peg reserves like
    ///      the live venue), allowlist the Xylo factory, fund the user, and
    ///      return the pool + the venue-only output (fee on output, 4 bps).
    function _prepareXyloSwap(uint256 amountIn)
        internal
        returns (MockXyloPool pool, uint256 expectedOut)
    {
        // Live XyloNet shape: ~9.3M USDC / ~0.6M EURC stored reserves.
        uint256 r0 = 9_323_185e6;
        uint256 r1 = 613_516e6;
        // Mint the seed to this test (the factory pulls it into the pool).
        usdc.mint(address(this), r0);
        eurc.mint(address(this), r1);
        usdc.approve(address(xyloFactory), type(uint256).max);
        eurc.approve(address(xyloFactory), type(uint256).max);
        address poolAddr = xyloFactory.createPool(address(usdc), address(eurc), r0, r1);
        agg.addFactory(address(xyloFactory), Aggregator.DexType.Xylo);

        pool = MockXyloPool(poolAddr);
        uint256 reserveIn = pool.token0() == address(usdc) ? r0 : r1;
        uint256 reserveOut = pool.token0() == address(usdc) ? r1 : r0;
        uint256 gross = (amountIn * reserveOut) / (reserveIn + amountIn);
        expectedOut = gross - (gross * 4) / 10_000;

        _fundUser(address(usdc), amountIn);
        _userApproveMockPermit2(address(usdc), type(uint256).max);
        _grantMockAllowance(address(usdc), uint160(amountIn));
    }

    /// @dev T-XYLO happy path: approve + `swap(...)` with `to = aggregator`.
    function test_xylo_hop_succeeds_via_transferfrom_swap() public {
        (MockXyloPool pool, uint256 expectedOut) = _prepareXyloSwap(1000e6);

        Aggregator.Hop memory h =
            _hop(address(pool), Aggregator.DexType.Xylo, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);

        vm.prank(user);
        uint256 amountOut = agg.splitSwap(
            address(usdc), address(eurc), 1000e6, expectedOut, block.timestamp, routes, _emptyPermit()
        );

        assertEq(amountOut, expectedOut, "venue-only Xylo math violated");
        assertEq(eurc.balanceOf(user), expectedOut, "user did not receive output");
        assertEq(pool.swapCount(), 1, "Xylo swap must have executed");
        // The aggregator's approval to the pool is reset to 0 (no dangling allowance).
        assertEq(usdc.allowance(address(agg), address(pool)), 0, "allowance must reset to 0");
        _assertCatalogZero();
    }

    /// @dev T-XYLO: an unknown Xylo factory (not allowlisted) reverts.
    function test_xylo_hop_unknown_factory_reverts() public {
        MockXyloFactory rogueFactory = new MockXyloFactory();
        usdc.mint(address(this), 1e6);
        eurc.mint(address(this), 1e6);
        usdc.approve(address(rogueFactory), type(uint256).max);
        eurc.approve(address(rogueFactory), type(uint256).max);
        address roguePool = rogueFactory.createPool(address(usdc), address(eurc), 1e6, 1e6);
        agg.addFactory(address(xyloFactory), Aggregator.DexType.Xylo);

        Aggregator.Hop memory h =
            _hop(roguePool, Aggregator.DexType.Xylo, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
        vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    /// @dev T-XYLO: a pool created by the allowlisted factory but for the
    ///      wrong pair (e.g. USDC/USYC — out of catalog) never matches a
    ///      catalog hop.
    function test_xylo_usyc_pool_never_matches_catalog_hop() public {
        address usyc = address(0xCC205224862C7641930c87679E98999d23C26113);
        MockErc20 usycToken = new MockErc20("USYC", "USYC");
        usdc.mint(address(this), 1e6);
        usycToken.mint(address(this), 1e6);
        usdc.approve(address(xyloFactory), type(uint256).max);
        usycToken.approve(address(xyloFactory), type(uint256).max);
        address usycPool = xyloFactory.createPool(address(usdc), address(usycToken), 1e6, 1e6);
        agg.addFactory(address(xyloFactory), Aggregator.DexType.Xylo);
        // Clean up the 1 USDC seed so the catalog-zero invariant stays valid
        // (the pool holds USDC out of the aggregator's reach, but the test
        // contract's approval is spent — the pool balance is irrelevant to
        // the aggregator).
        vm.prank(address(this));
        usdc.approve(address(xyloFactory), 0);
        vm.prank(address(this));
        usycToken.approve(address(xyloFactory), 0);

        Aggregator.Hop memory h =
            _hop(usycPool, Aggregator.DexType.Xylo, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
        vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    /// @dev T-XYLO: the aggregator must NOT call `exchange(i,j)` on a Xylo
    ///      pool (different ABI). Routing a Xylo pool as `DexType.Stable`
    ///      fails the factory membership (Xylo factory is allowlisted as
    ///      Xylo only) — `exchange` is never reached.
    function test_xylo_pool_not_usable_as_stable_hop() public {
        usdc.mint(address(this), 9_323_185e6);
        eurc.mint(address(this), 613_516e6);
        usdc.approve(address(xyloFactory), type(uint256).max);
        eurc.approve(address(xyloFactory), type(uint256).max);
        address poolAddr = xyloFactory.createPool(address(usdc), address(eurc), 9_323_185e6, 613_516e6);
        agg.addFactory(address(xyloFactory), Aggregator.DexType.Xylo);

        // Xylo factory is NOT allowlisted as Stable → PoolNotFromFactory.
        Aggregator.Hop memory h =
            _hop(poolAddr, Aggregator.DexType.Stable, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);
        vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 1000e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    /// @dev T-XYLO: `removeFactory` gates Xylo hops like every other venue.
    function test_removeFactory_gates_xylo_hops() public {
        usdc.mint(address(this), 9_323_185e6);
        eurc.mint(address(this), 613_516e6);
        usdc.approve(address(xyloFactory), type(uint256).max);
        eurc.approve(address(xyloFactory), type(uint256).max);
        address poolAddr = xyloFactory.createPool(address(usdc), address(eurc), 9_323_185e6, 613_516e6);
        agg.addFactory(address(xyloFactory), Aggregator.DexType.Xylo);
        agg.removeFactory(address(xyloFactory));

        Aggregator.Hop memory h =
            _hop(poolAddr, Aggregator.DexType.Xylo, address(usdc), address(eurc));
        Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 1000e6);
        vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
        agg.splitSwap(
            address(usdc), address(eurc), 1000e6, 0, block.timestamp, routes, _emptyPermit()
        );
    }

    // ─── 12. Never-call table ─────────────────────────────────

    function test_never_call_addresses_not_allowlisted() public view {
        for (uint256 i = 0; i < neverCall.length; i++) {
            assertFalse(agg.isFactory(neverCall[i]), "never-call address allowlisted");
        }
    }

    function test_never_call_hop_reverts_empty_allowlist() public {
        for (uint256 i = 0; i < neverCall.length; i++) {
            Aggregator.Hop memory h =
                _hop(neverCall[i], Aggregator.DexType.Xyk, address(usdc), address(eurc));
            Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
            vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
            agg.splitSwap(
                address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
            );
        }
    }

    function test_never_call_hop_reverts_after_allowlisting() public {
        _seedXykPair(address(usdc), address(eurc), XYK_SEED, XYK_SEED);
        agg.addFactory(address(xykFactory), Aggregator.DexType.Xyk);
        agg.addFactory(address(stableFactory), Aggregator.DexType.Stable);
        agg.addFactory(address(clmmFactory), Aggregator.DexType.Clmm);
        for (uint256 i = 0; i < neverCall.length; i++) {
            Aggregator.Hop memory h =
                _hop(neverCall[i], Aggregator.DexType.Clmm, address(usdc), address(eurc));
            h.fee = 3000;
            Aggregator.SubRoute[] memory routes = _singleHopRoute(h, 100e6);
            vm.expectRevert(Aggregator.PoolNotFromFactory.selector);
            agg.splitSwap(
                address(usdc), address(eurc), 100e6, 0, block.timestamp, routes, _emptyPermit()
            );
        }
    }

    // ─── Helpers ──────────────────────────────────────────────

    function _callSplitSwap(
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 minOut,
        uint256 deadline
    ) internal {
        agg.splitSwap(
            tokenIn,
            tokenOut,
            amountIn,
            minOut,
            deadline,
            new Aggregator.SubRoute[](0),
            _emptyPermit()
        );
    }

    function _emptyPermit() internal pure returns (Aggregator.Permit2Pull memory p) {
        // All-zero PermitSingle + empty signature → aggregator skips permit().
    }

    function _singleHopRoute(Aggregator.Hop memory hop, uint256 amountIn)
        internal
        pure
        returns (Aggregator.SubRoute[] memory routes)
    {
        routes = new Aggregator.SubRoute[](1);
        routes[0].amountIn = amountIn;
        routes[0].hops = new Aggregator.Hop[](1);
        routes[0].hops[0] = hop;
    }

    function _hop(address pool, Aggregator.DexType dexType, address tokenIn, address tokenOut)
        internal
        pure
        returns (Aggregator.Hop memory h)
    {
        h.pool = pool;
        h.dexType = dexType;
        h.tokenIn = tokenIn;
        h.tokenOut = tokenOut;
    }

    function _assertCatalogZero() internal view {
        assertEq(usdc.balanceOf(address(agg)), 0, "USDC leftover");
        assertEq(eurc.balanceOf(address(agg)), 0, "EURC leftover");
        assertEq(mbtc.balanceOf(address(agg)), 0, "mBTC leftover");
    }

    function _seedXykPair(address tokenA, address tokenB, uint256 amountA, uint256 amountB)
        internal
        returns (address pair)
    {
        pair = xykFactory.createPair(tokenA, tokenB);
        _mintAndTransferXyk(tokenA, pair, amountA);
        _mintAndTransferXyk(tokenB, pair, amountB);
        IUniswapV2Pair(pair).mint(alice);
    }

    function _xykFormula(IUniswapV2Pair p, address tokenIn, uint256 amountIn)
        internal
        view
        returns (uint256)
    {
        bool inIsToken0 = p.token0() == tokenIn;
        (uint112 r0, uint112 r1,) = p.getReserves();
        uint256 reserveIn = inIsToken0 ? uint256(r0) : uint256(r1);
        uint256 reserveOut = inIsToken0 ? uint256(r1) : uint256(r0);
        return (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997);
    }

    function _seedStablePool() internal returns (address pool) {
        pool = stableFactory.createPool(address(usdc), address(eurc));
        StableSwap s = StableSwap(pool);
        _mintAndTransferStable(s.token0(), pool, STABLE_SEED);
        _mintAndTransferStable(s.token1(), pool, STABLE_SEED);
        s.seedLiquidity(STABLE_SEED, STABLE_SEED);
    }

    function _mintAndTransferStable(address token, address to, uint256 amount) internal {
        MockErc20(token).mint(address(this), amount);
        MockErc20(token).transfer(to, amount);
    }

    /// @dev Seed EURC/USDC + USDC/mBTC pairs, allowlist xyk, fund user with EURC.
    function _prepareMultiHop()
        internal
        returns (address pairEurcUsdc, address pairUsdcMbtc, uint256 expectedOut)
    {
        pairEurcUsdc = _seedXykPair(address(eurc), address(usdc), XYK_SEED, XYK_SEED);
        pairUsdcMbtc = _seedXykPair(address(usdc), address(mbtc), 50_000e6, 1e8);
        agg.addFactory(address(xykFactory), Aggregator.DexType.Xyk);

        _fundUser(address(eurc), 1000e6);
        _userApproveMockPermit2(address(eurc), type(uint256).max);
        _grantMockAllowance(address(eurc), 1000e6);

        uint256 out1 = _xykFormula(IUniswapV2Pair(pairEurcUsdc), address(eurc), 1000e6);
        expectedOut = _xykFormula(IUniswapV2Pair(pairUsdcMbtc), address(usdc), out1);
    }

    /// @dev Seed thin xyk + deep stable USDC/EURC, allowlist both, fund user with USDC.
    function _prepareSplit() internal returns (address xykPair, address stablePool) {
        xykPair = _seedXykPair(address(usdc), address(eurc), XYK_SEED, XYK_SEED);
        agg.addFactory(address(xykFactory), Aggregator.DexType.Xyk);
        stablePool = _seedStablePool();
        agg.addFactory(address(stableFactory), Aggregator.DexType.Stable);

        _fundUser(address(usdc), 1000e6);
        _userApproveMockPermit2(address(usdc), type(uint256).max);
        _grantMockAllowance(address(usdc), 1000e6);
    }

    /// @dev Full-range L=1e12 CLMM pool (T2.4 fixture), allowlisted, user funded.
    function _prepareClmm() internal returns (IUniswapV3Pool pool) {
        address poolAddr = clmmFactory.createPool(address(usdc), address(mbtc), 3000);
        pool = IUniswapV3Pool(poolAddr);
        pool.initialize(792281625142643375935439503360); // sqrtPriceX96 = 10 * 2^96 (P=100)

        usdc.mint(address(this), 100_000e6);
        mbtc.mint(address(this), 100_000e8);
        pool.mint(alice, -887220, 887220, 1e12, "");

        agg.addFactory(address(clmmFactory), Aggregator.DexType.Clmm);
        _fundUser(address(usdc), 100e6);
        _userApproveMockPermit2(address(usdc), type(uint256).max);
        _grantMockAllowance(address(usdc), 100e6);
    }

    function _mintAndTransferXyk(address token, address to, uint256 amount) internal {
        if (token == address(mbtc)) {
            mbtc.mint(address(this), amount);
            mbtc.transfer(to, amount);
        } else {
            MockErc20(token).mint(address(this), amount);
            MockErc20(token).transfer(to, amount);
        }
    }

    // ─── Permit2 / funding helpers ────────────────────────────

    /// @dev Seed allowlisted xyk pair + fund user + approve MockPermit2.
    ///      Returns the pair and the venue-only expected output (997/1000).
    function _prepareXykSwap(uint256 amountIn)
        internal
        returns (address pair, uint256 expectedOut)
    {
        pair = _seedXykPair(address(usdc), address(eurc), XYK_SEED, XYK_SEED);
        agg.addFactory(address(xykFactory), Aggregator.DexType.Xyk);

        _fundUser(address(usdc), amountIn);
        _userApproveMockPermit2(address(usdc), type(uint256).max);

        IUniswapV2Pair p = IUniswapV2Pair(pair);
        bool usdcIsToken0 = p.token0() == address(usdc);
        (uint112 r0, uint112 r1,) = p.getReserves();
        uint256 reserveIn = usdcIsToken0 ? uint256(r0) : uint256(r1);
        uint256 reserveOut = usdcIsToken0 ? uint256(r1) : uint256(r0);
        expectedOut = (amountIn * 997 * reserveOut) / (reserveIn * 1000 + amountIn * 997);
    }

    function _fundUser(address token, uint256 amount) internal {
        if (token == address(mbtc)) {
            mbtc.mint(user, amount);
        } else {
            MockErc20(token).mint(user, amount);
        }
    }

    function _userApproveMockPermit2(address token, uint256 amount) internal {
        vm.prank(user);
        if (token == address(mbtc)) {
            mbtc.approve(address(permit2), amount);
        } else {
            MockErc20(token).approve(address(permit2), amount);
        }
    }

    function _grantMockAllowance(address token, uint160 amount) internal {
        permit2.approve(user, token, address(agg), amount, uint48(block.timestamp + 1 days));
    }

    function _signedPull(uint256 amountIn, bytes memory signature)
        internal
        view
        returns (Aggregator.Permit2Pull memory pull)
    {
        pull.permitSingle = IAllowanceTransfer.PermitSingle({
            details: IAllowanceTransfer.PermitDetails({
                token: address(usdc),
                amount: uint160(amountIn),
                expiration: uint48(block.timestamp + 10 minutes),
                nonce: 0
            }),
            spender: address(agg),
            sigDeadline: block.timestamp + 10 minutes
        });
        pull.signature = signature;
    }
}

/// @notice Mimics a V3 pool's token0/token1/fee view surface, then spoofs the
///      aggregator's uniswapV3SwapCallback from a non-allowlisted address.
contract FakePool {
    Aggregator public immutable agg;
    address public immutable fakeToken0;
    address public immutable fakeToken1;
    uint24 public immutable fee;

    constructor(address _token0, address _token1, uint24 _fee, Aggregator _agg) {
        fakeToken0 = _token0;
        fakeToken1 = _token1;
        fee = _fee;
        agg = _agg;
    }

    function token0() external view returns (address) {
        return fakeToken0;
    }

    function token1() external view returns (address) {
        return fakeToken1;
    }

    function spoof() external {
        agg.uniswapV3SwapCallback(1e6, 0, abi.encode(address(this), fakeToken0));
    }
}
