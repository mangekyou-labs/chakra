// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Test} from "forge-std/Test.sol";
import {MockBtc} from "../src/MockBtc.sol";

contract MockBtcTest is Test {
    MockBtc internal token;
    address internal owner = address(this);
    address internal stranger = address(0xB0B);

    function setUp() public {
        token = new MockBtc();
    }

    function test_decimals_are_eight() public view {
        assertEq(token.decimals(), 8);
    }

    function test_name_and_symbol() public view {
        assertEq(token.name(), "Mock BTC");
        assertEq(token.symbol(), "mBTC");
    }

    function test_owner_can_mint() public {
        token.mint(stranger, 1e8);
        assertEq(token.balanceOf(stranger), 1e8);
        assertEq(token.totalSupply(), 1e8);
    }

    function test_non_owner_cannot_mint() public {
        vm.prank(stranger);
        vm.expectRevert(MockBtc.NotOwner.selector);
        token.mint(stranger, 1);
    }

    function test_no_public_faucet() public {
        // Public mint path is owner-only; a faucet would be permissionless.
        // Non-owner mint is the faucet-equivalent and must revert.
        vm.prank(stranger);
        vm.expectRevert(MockBtc.NotOwner.selector);
        token.mint(stranger, 100e8);
    }
}
