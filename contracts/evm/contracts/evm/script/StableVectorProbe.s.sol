// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;
import {Test} from "forge-std/Test.sol";
import {StableSwap} from "../src/stable/StableSwap.sol";
import {MockErc20} from "../src/MockErc20.sol";
import {StableSwapFactory} from "../src/stable/StableSwapFactory.sol";

contract StableVectorProbe is Test {
    function run() external {
        MockErc20 usdc = new MockErc20("USDC", "USDC", 6);
        MockErc20 eurc = new MockErc20("EURC", "EURC", 6);
        StableSwapFactory factory = new StableSwapFactory();
        address poolAddr = factory.createPool(address(usdc), address(eurc));
        StableSwap pool = StableSwap(poolAddr);
        usdc.mint(address(this), 200_000e6);
        eurc.mint(address(this), 200_000e6);
        usdc.transfer(address(pool), 200_000e6);
        eurc.transfer(address(pool), 200_000e6);
        pool.seedLiquidity(200_000e6, 200_000e6);
        for (uint256 i = 0; i < 3; i++) {
            usdc.mint(address(this), 1_000e6);
            usdc.transfer(address(pool), 1_000e6);
            uint256 balBefore = eurc.balanceOf(address(this));
            pool.exchange(0, 1, 1_000e6, 0);
            uint256 dy = eurc.balanceOf(address(this)) - balBefore;
            emit log_named_uint("probe_dy", dy);
        }
    }
}
