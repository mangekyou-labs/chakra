// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {Aggregator} from "../src/Aggregator.sol";
import {MockBtc} from "../src/MockBtc.sol";
import {StableSwapFactory} from "../src/stable/StableSwapFactory.sol";
import {StableSwap} from "../src/stable/StableSwap.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";
import {LiquiditySeeder} from "../src/LiquiditySeeder.sol";
import {IUniswapV2Factory} from "../src/interfaces/IUniswapV2Factory.sol";
import {IUniswapV2Pair} from "../src/interfaces/IUniswapV2Pair.sol";
import {IUniswapV3Factory} from "../src/interfaces/IUniswapV3Factory.sol";
import {IUniswapV3Pool} from "../src/interfaces/IUniswapV3Pool.sol";

/// @notice Rerun-aware liquidity seeding for Arc testnet.
/// @dev Prerequisites: Deploy.s.sol must have run; all CHAKRA_* addresses
///      must be set.  Idempotent: skips already-funded V2/stable pools and
///      matching initialized V3 liquidity.  Aborts on incompatible state.
///
///      Allocation (usable ERC20 USDC after reserving gas):
///        - 40% stable USDC/EURC
///        - 20% V2 USDC/EURC
///        - 20% V2 USDC/mBTC
///        - 20% V3 USDC/mBTC
///      EURC allocation:
///        - 50% stable USDC/EURC
///        - 25% V2 USDC/EURC
///        - 25% V2 EURC/mBTC
///
///      mBTC target: configurable via CHAKRA_MBTC_TARGET (default 10 mBTC = 10e8).
///      Min per side: 0.10 USDC/EURC = 100_000 (6 dp).
///      V3: 30 bps fee, reference price 100,000 USDC per BTC, active range ±6000 ticks.
contract Seed is Script, VendorDeployer {
    uint256 internal constant CHAKRA_CHAIN_ID = 5042002;
    uint256 internal constant GAS_RESERVE_USDC = 200_000; // 0.20 USDC native gas reserve

    // ─── Token addresses (from env, same as Deploy.s.sol) ─────
    address internal usdc;
    address internal eurc;
    address internal mbtc;
    address internal permit2;
    address internal xykFactory;
    address internal stableFactory;
    address internal clmmFactory;
    address internal aggregator;
    address internal operator;
    LiquiditySeeder internal liquiditySeeder;

    // ─── CLMM constants ────────────────────────────────────────
    uint24 internal constant CLMM_FEE = 3000; // 30 bps
    uint160 internal constant REF_SQRT_USDC_TOKEN0 = 2505414483750479311864138015;
    uint160 internal constant REF_SQRT_MBTC_TOKEN0 = 2505414483750479311864138015696;
    int24 internal constant FULL_RANGE_LOWER = -887220;
    int24 internal constant FULL_RANGE_UPPER = 887220;

    function run() external {
        require(block.chainid == CHAKRA_CHAIN_ID, "Seed: chain must be Arc testnet (5042002)");

        console.log("=== Arc Testnet Seed (rerun-aware) ===");

        _loadAddresses();
        operator = vm.addr(vm.envUint("PRIVATE_KEY"));
        _preflight();
        _loadOrDeployLiquiditySeeder();

        // ── 1. Mint mBTC up to target ──────────────────────────
        _seedMbtc();

        // ── 2. Compute USDC allocation ─────────────────────────
        uint256 nativeUsdc = IERC20(usdc).balanceOf(operator);
        uint256 gasReserve = _max(2 * GAS_RESERVE_USDC, 2 * GAS_RESERVE_USDC);
        require(nativeUsdc > gasReserve, "Insufficient USDC for gas reserve");
        uint256 usableUsdc = nativeUsdc - gasReserve;
        console.log("  Native USDC balance:", nativeUsdc);
        console.log("  Gas reserve:", gasReserve);
        console.log("  Usable USDC:", usableUsdc);

        uint256 eurcBal = IERC20(eurc).balanceOf(operator);
        console.log("  EURC balance:", eurcBal);

        // Verify minimum per side (0.10 USDC/EURC = 100_000).
        uint256 minPerSide = 100_000;

        // ── USDC allocation (40/20/20/20) ─────────────────────
        uint256 usdcStable = usableUsdc * 40 / 100;
        uint256 usdcV2Eurc = usableUsdc * 20 / 100;
        uint256 usdcV2Mbtc = usableUsdc * 20 / 100;
        uint256 usdcV3Mbtc = usableUsdc - usdcStable - usdcV2Eurc - usdcV2Mbtc;

        require(
            usdcStable >= minPerSide && usdcV2Eurc >= minPerSide && usdcV2Mbtc >= minPerSide
                && usdcV3Mbtc >= minPerSide,
            "Insufficient USDC per venue side (min 0.10 USDC)"
        );

        // ── EURC allocation (50/25/25) ────────────────────────
        // EURC needed: stable (50%), V2 USDC/EURC (25%), V2 EURC/mBTC (25%)
        // For stable: EURC ≈ USDC (balanced)
        // For V2 USDC/EURC: EURC ≈ USDC (balanced)
        // For V2 EURC/mBTC: EURC only side
        uint256 eurcStable = usdcStable; // balanced stable
        uint256 eurcV2Eurc = usdcV2Eurc; // balanced xyk
        uint256 eurcV2Mbtc = usdcV2Mbtc; // EURC side of EURC/mBTC pair

        require(eurcBal >= eurcStable + eurcV2Eurc + eurcV2Mbtc, "Insufficient EURC balance");

        console.log("  USDC allocation:");
        console.log("    stable:", usdcStable, "v2Eurc:", usdcV2Eurc);
        console.log("    v2Mbtc:", usdcV2Mbtc, "v3Mbtc:", usdcV3Mbtc);
        console.log("  EURC allocation:");
        console.log("    stable:", eurcStable, "v2Eurc:", eurcV2Eurc);
        console.log("    v2Mbtc:", eurcV2Mbtc);

        // ── 3. Stable USDC/EURC ────────────────────────────────
        _seedStable(usdcStable, eurcStable, minPerSide);

        // ── 4. V2 USDC/EURC ───────────────────────────────────
        _seedV2(usdc, eurc, usdcV2Eurc, eurcV2Eurc, minPerSide);

        // ── 5. V2 USDC/mBTC ───────────────────────────────────
        uint256 mbtcV2Mbtc = usdcV2Mbtc / 1000;
        _seedV2(usdc, mbtc, usdcV2Mbtc, mbtcV2Mbtc, minPerSide / 1000);

        // ── 6. V2 EURC/mBTC ───────────────────────────────────
        uint256 mbtcV2Eurc = eurcV2Mbtc / 1000;
        _seedV2(eurc, mbtc, eurcV2Mbtc, mbtcV2Eurc, minPerSide / 1000);

        // ── 7. V3 USDC/mBTC (30 bps, init price 100k) ─────────
        _seedV3(usdc, mbtc, usdcV3Mbtc, usdcV3Mbtc / 1000);

        // ── 8. Allowlist stable pools in aggregator ────────────
        _allowlistStablePools();

        console.log("");
        console.log("=== Seed Complete ===");
    }

    function _loadAddresses() internal {
        usdc = vm.envOr("CHAKRA_USDC_ADDRESS", address(0x3600000000000000000000000000000000000000));
        eurc = vm.envOr("CHAKRA_EURC_ADDRESS", address(0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a));
        mbtc = vm.envAddress("CHAKRA_MBTC_ADDRESS");
        permit2 = vm.envOr("CHAKRA_PERMIT2", address(0x000000000022D473030F116dDEE9F6B43aC78BA3));
        xykFactory = vm.envAddress("CHAKRA_XYK_FACTORY");
        stableFactory = vm.envAddress("CHAKRA_STABLE_FACTORY");
        clmmFactory = vm.envAddress("CHAKRA_CLMM_FACTORY");
        aggregator = vm.envAddress("CHAKRA_AGGREGATOR");
        require(mbtc != address(0), "CHAKRA_MBTC_ADDRESS required");
        require(xykFactory != address(0), "CHAKRA_XYK_FACTORY required");
        require(stableFactory != address(0), "CHAKRA_STABLE_FACTORY required");
        require(clmmFactory != address(0), "CHAKRA_CLMM_FACTORY required");
        require(aggregator != address(0), "CHAKRA_AGGREGATOR required");
    }

    function _preflight() internal view {
        require(_hasCode(usdc), "USDC: no code at address");
        require(_hasCode(eurc), "EURC: no code at address");
        require(_hasCode(mbtc), "mBTC: no code at address");
        require(_hasCode(xykFactory), "XYK factory: no code at address");
        require(_hasCode(stableFactory), "Stable factory: no code at address");
        require(_hasCode(clmmFactory), "CLMM factory: no code at address");
        require(_hasCode(aggregator), "Aggregator: no code at address");
        require(operator != address(0), "Deployer must be EOA or contract");
    }

    function _loadOrDeployLiquiditySeeder() internal {
        address configured = vm.envOr("CHAKRA_LIQUIDITY_SEEDER", address(0));
        if (configured != address(0)) {
            require(_hasCode(configured), "LiquiditySeeder: no code at configured address");
            liquiditySeeder = LiquiditySeeder(configured);
            console.log("  Reusing LiquiditySeeder:", configured);
            return;
        }
        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        liquiditySeeder = new LiquiditySeeder();
        vm.stopBroadcast();
        console.log("  CHAKRA_LIQUIDITY_SEEDER=", address(liquiditySeeder));
    }

    function _seedMbtc() internal {
        uint256 target = vm.envOr("CHAKRA_MBTC_TARGET", uint256(10 ether)); // default 10 mBTC
        // mBTC uses 8 decimals, so 10 ether = 10e8
        // Scale target to 8 decimals if env is in ether (18 dp).
        if (target > 1e15) {
            target = target / 1e10; // 18dp -> 8dp
        }
        uint256 current = MockBtc(mbtc).balanceOf(operator);
        if (current >= target) {
            console.log("[1/8] mBTC already funded:", current, ">= target", target);
            return;
        }
        uint256 deficit = target - current;
        console.log("[1/8] Minting mBTC deficit:", deficit);
        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        MockBtc(mbtc).mint(operator, deficit);
        vm.stopBroadcast();
        console.log("  mBTC minted:", deficit, "new balance:", MockBtc(mbtc).balanceOf(operator));
    }

    function _seedStable(uint256 usdcAmt, uint256 eurcAmt, uint256 minPerSide) internal {
        require(usdcAmt >= minPerSide && eurcAmt >= minPerSide, "Stable: below min per side");

        // Check if stable pool already exists and is funded.
        address existingPool = StableSwapFactory(stableFactory).getPool(usdc, eurc);
        if (existingPool != address(0)) {
            uint256 poolUsdc = IERC20(usdc).balanceOf(existingPool);
            if (poolUsdc >= usdcAmt / 2) {
                console.log("[2/8] Stable USDC/EURC pool already funded:", existingPool);
                return;
            }
            console.log("[2/8] Stable pool exists but underfunded, skipping re-seed");
            return;
        }

        console.log("[2/8] Creating Stable USDC/EURC pool...");
        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        address poolAddr = StableSwapFactory(stableFactory).createPool(usdc, eurc);
        vm.stopBroadcast();

        // Transfer tokens and seed.
        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        IERC20(usdc).transfer(poolAddr, usdcAmt);
        IERC20(eurc).transfer(poolAddr, eurcAmt);
        vm.stopBroadcast();

        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        StableSwap(poolAddr).seedLiquidity(usdcAmt, eurcAmt);
        vm.stopBroadcast();

        console.log("  Stable pool:", poolAddr);
        console.log("  seeded USDC:", usdcAmt, "EURC:", eurcAmt);
    }

    function _seedV2(address tokenA, address tokenB, uint256 amtA, uint256 amtB, uint256 minPerSide)
        internal
    {
        require(amtA >= minPerSide && amtB >= minPerSide, "V2: below min per side");

        address pair = IUniswapV2Factory(xykFactory).getPair(tokenA, tokenB);
        if (pair != address(0)) {
            (uint112 reserve0, uint112 reserve1,) = IUniswapV2Pair(pair).getReserves();
            if (reserve0 != 0 && reserve1 != 0) {
                console.log("[V2] Pair already funded:", pair);
                return;
            }
        }

        string memory labelA = _tokenLabel(tokenA);
        string memory labelB = _tokenLabel(tokenB);
        console.log("[V2] Creating pair:");
        console.log("  ", labelA, "/", labelB);

        if (pair == address(0)) {
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            pair = IUniswapV2Factory(xykFactory).createPair(tokenA, tokenB);
            vm.stopBroadcast();
        }

        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        IERC20(tokenA).approve(address(liquiditySeeder), amtA);
        IERC20(tokenB).approve(address(liquiditySeeder), amtB);
        uint256 lpTokens = liquiditySeeder.seedV2(pair, tokenA, tokenB, amtA, amtB, operator);
        vm.stopBroadcast();

        console.log("  V2 pair:", pair, "seeded:");
        console.log("  ", labelA, "=", amtA);
        console.log("  ", labelB, "=", amtB);
        console.log("  LP minted:", lpTokens);
    }

    function _seedV3(address tokenA, address tokenB, uint256 usdcAmt, uint256 mbtcAmt) internal {
        require(usdcAmt > 0 && mbtcAmt > 0, "V3: amounts must be positive");

        address poolAddr = IUniswapV3Factory(clmmFactory).getPool(tokenA, tokenB, CLMM_FEE);
        if (poolAddr != address(0)) {
            IUniswapV3Pool.Slot0 memory s0 = IUniswapV3Pool(poolAddr).slot0();
            if (s0.sqrtPriceX96 != 0 && IUniswapV3Pool(poolAddr).liquidity() != 0) {
                console.log("[V3] Pool already funded:", poolAddr);
                return;
            }
        }

        console.log("[V3] Creating USDC/mBTC pool at 30 bps...");

        if (poolAddr == address(0)) {
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            poolAddr = IUniswapV3Factory(clmmFactory).createPool(tokenA, tokenB, CLMM_FEE);
            vm.stopBroadcast();
        }

        IUniswapV3Pool pool = IUniswapV3Pool(poolAddr);
        uint160 sqrtPriceX96 = pool.token0() == tokenA ? REF_SQRT_USDC_TOKEN0 : REF_SQRT_MBTC_TOKEN0;
        if (pool.slot0().sqrtPriceX96 == 0) {
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            pool.initialize(sqrtPriceX96);
            vm.stopBroadcast();
        }

        uint128 liquidity = uint128(usdcAmt / 64);
        require(liquidity != 0, "V3: liquidity rounds to zero");
        uint256 amount0Max = pool.token0() == tokenA ? usdcAmt : mbtcAmt;
        uint256 amount1Max = pool.token0() == tokenA ? mbtcAmt : usdcAmt;
        vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
        IERC20(tokenA).approve(address(liquiditySeeder), usdcAmt);
        IERC20(tokenB).approve(address(liquiditySeeder), mbtcAmt);
        (uint256 amount0, uint256 amount1) = liquiditySeeder.seedV3(
            poolAddr,
            FULL_RANGE_LOWER,
            FULL_RANGE_UPPER,
            liquidity,
            amount0Max,
            amount1Max,
            operator
        );
        vm.stopBroadcast();

        console.log("  V3 pool initialized. sqrtPriceX96:", sqrtPriceX96);
        console.log("  Active full-range liquidity:", liquidity);
        console.log("  token0 seeded:", amount0, "token1 seeded:", amount1);
    }

    function _allowlistStablePools() internal {
        console.log("[8/8] Allowlisting stable pools in aggregator...");
        address stablePool = StableSwapFactory(stableFactory).getPool(usdc, eurc);
        if (stablePool != address(0)) {
            vm.startBroadcast(vm.envUint("PRIVATE_KEY"));
            Aggregator(payable(aggregator)).addStablePool(stablePool);
            vm.stopBroadcast();
            console.log("  Allowlisted stable pool:", stablePool);
        }
    }

    function _tokenLabel(address token) internal view returns (string memory) {
        if (token == usdc) return "USDC";
        if (token == eurc) return "EURC";
        if (token == mbtc) return "mBTC";
        return "UNKNOWN";
    }

    function _max(uint256 a, uint256 b) internal pure returns (uint256) {
        return a > b ? a : b;
    }

    function _hasCode(address addr) internal view returns (bool) {
        uint256 size;
        assembly {
            size := extcodesize(addr)
        }
        return size > 0;
    }
}
