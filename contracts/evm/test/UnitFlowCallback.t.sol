// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {Aggregator} from "../src/Aggregator.sol";
import {MockErc20} from "../src/MockErc20.sol";
import {MockPermit2} from "./MockPermit2.sol";

// Matches the deployed UnitFlow pool's callback selector (capital F).
contract UnitFlowPoolFixture {
    address public token0;
    address public token1;
    uint24 public constant fee = 100;
    Aggregator public aggregator;
    constructor(address a, address b, Aggregator agg) { token0 = a; token1 = b; aggregator = agg; }
    function swap(address recipient, bool zeroForOne, int256 amount, uint160, bytes calldata data)
        external returns (int256, int256)
    {
        require(zeroForOne);
        MockErc20(token1).transfer(recipient, uint256(amount));
        aggregator.unitFlowV3SwapCallback(amount, -amount, data);
        return (amount, -amount);
    }
    function spoof() external {
        aggregator.unitFlowV3SwapCallback(1, 0, abi.encode(address(this), token0));
    }
}
contract UnitFlowFactoryFixture {
    address public pool;
    function setPool(address p) external { pool = p; }
    function getPool(address, address, uint24 fee) external view returns (address) {
        return fee == 100 ? pool : address(0);
    }
}
contract UnitFlowCallbackTest is Test {
    Aggregator agg;
    MockErc20 usdc;
    MockErc20 eurc;
    UnitFlowPoolFixture pool;
    function setUp() public {
        usdc = new MockErc20("USDC", "USDC");
        eurc = new MockErc20("EURC", "EURC");
        MockErc20 btc = new MockErc20("BTC", "BTC");
        MockPermit2 permit = new MockPermit2();
        agg = new Aggregator(address(permit), address(usdc), address(eurc), address(btc));
        pool = new UnitFlowPoolFixture(address(usdc), address(eurc), agg);
        UnitFlowFactoryFixture factory = new UnitFlowFactoryFixture();
        factory.setPool(address(pool));
        agg.addFactory(address(factory), Aggregator.DexType.Clmm);
        usdc.mint(address(this), 100_000);
        eurc.mint(address(pool), 100_000);
        usdc.approve(address(permit), 100_000);
        permit.approve(address(this), address(usdc), address(agg), 100_000, uint48(block.timestamp + 120));
    }
    function test_unitFlow_callback_executes_complete_swap() public {
        Aggregator.Hop[] memory hops = new Aggregator.Hop[](1);
        hops[0] = Aggregator.Hop(address(pool), Aggregator.DexType.Clmm, address(usdc), address(eurc), 100);
        Aggregator.SubRoute[] memory routes = new Aggregator.SubRoute[](1);
        routes[0] = Aggregator.SubRoute(100_000, hops);
        Aggregator.Permit2Pull memory permit;
        uint256 out = agg.splitSwap(address(usdc), address(eurc), 100_000, 99_000, block.timestamp, routes, permit);
        assertEq(out, 100_000);
        assertEq(eurc.balanceOf(address(this)), out);
        assertEq(usdc.balanceOf(address(pool)), 100_000);
        assertEq(usdc.balanceOf(address(agg)), 0);
    }
    function test_unitFlow_callback_rejects_spoofed_sender() public {
        vm.expectRevert(Aggregator.CallbackSenderMismatch.selector);
        agg.unitFlowV3SwapCallback(1, 0, abi.encode(address(pool), address(usdc)));
    }
    function test_unitFlow_callback_rejects_unregistered_pool() public {
        UnitFlowPoolFixture fake = new UnitFlowPoolFixture(address(usdc), address(eurc), agg);
        vm.expectRevert(Aggregator.CallbackPoolNotAllowlisted.selector);
        fake.spoof();
    }
}
