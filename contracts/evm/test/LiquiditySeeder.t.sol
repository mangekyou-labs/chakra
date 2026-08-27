// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {LiquiditySeeder} from "../src/LiquiditySeeder.sol";
import {VendorDeployer} from "../src/VendorDeployer.sol";
import {MockErc20} from "../src/MockErc20.sol";
import {MockBtc} from "../src/MockBtc.sol";
import {IUniswapV2Factory} from "../src/interfaces/IUniswapV2Factory.sol";
import {IUniswapV2Pair} from "../src/interfaces/IUniswapV2Pair.sol";
import {IUniswapV3Factory} from "../src/interfaces/IUniswapV3Factory.sol";
import {IUniswapV3Pool} from "../src/interfaces/IUniswapV3Pool.sol";

contract LiquiditySeederTest is Test, VendorDeployer {
    MockErc20 internal usdc;
    MockBtc internal mbtc;
    LiquiditySeeder internal seeder;

    function setUp() public {
        usdc = new MockErc20("USD Coin", "USDC");
        mbtc = new MockBtc();
        seeder = new LiquiditySeeder();
        usdc.mint(address(this), 1_000_000e6);
        mbtc.mint(address(this), 1_000_000e8);
        usdc.approve(address(seeder), type(uint256).max);
        mbtc.approve(address(seeder), type(uint256).max);
    }

    function test_seedV2_mintsLpAndInitializesReserves() public {
        IUniswapV2Factory factory = IUniswapV2Factory(
            _deployFromHexFileWithArgs("bytecodes/v2-factory.hex", abi.encode(address(0)))
        );
        IUniswapV2Pair pair = IUniswapV2Pair(factory.createPair(address(usdc), address(mbtc)));

        uint256 liquidity = seeder.seedV2(
            address(pair), address(usdc), address(mbtc), 10_000e6, 1e8, address(this)
        );

        assertGt(liquidity, 0, "LP not minted");
        (uint112 reserve0, uint112 reserve1,) = pair.getReserves();
        assertGt(reserve0, 0, "reserve0 not initialized");
        assertGt(reserve1, 0, "reserve1 not initialized");
    }

    function test_seedV3_paysMintCallbackAndCreatesActiveLiquidity() public {
        IUniswapV3Factory factory =
            IUniswapV3Factory(_deployFromHexFile("bytecodes/v3-factory.hex"));
        IUniswapV3Pool pool = IUniswapV3Pool(factory.createPool(address(usdc), address(mbtc), 3000));
        uint160 sqrtPriceX96 = pool.token0() == address(usdc)
            ? 2505414483750479311864138015  // sqrt(1 / 1000) * 2^96
            : 2505414483750479311864138015696; // sqrt(1000) * 2^96
        pool.initialize(sqrtPriceX96);

        (uint256 amount0, uint256 amount1) = seeder.seedV3(
            address(pool), -887220, 887220, 30_000, 1_000_000e6, 1_000_000e8, address(this)
        );

        assertGt(amount0, 0, "token0 callback amount missing");
        assertGt(amount1, 0, "token1 callback amount missing");
        assertGt(pool.liquidity(), 0, "V3 liquidity not active");
    }
}
