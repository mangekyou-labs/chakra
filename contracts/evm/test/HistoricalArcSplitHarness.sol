// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

import {Aggregator} from "../src/Aggregator.sol";
import {IAllowanceTransfer} from "../src/interfaces/IAllowanceTransfer.sol";
import {IXyloPool} from "../src/interfaces/IXyloNet.sol";

interface IPrestoReserves {
    function pathReserves(address token) external view returns (uint256);
    function tokenReserves(address token) external view returns (uint256);
}

/// @notice Read-only historical Arc split-swap harness. Its runtime bytecode is
/// injected at an unused address by an `eth_call` state override. It is never
/// deployed and cannot broadcast a transaction.
contract HistoricalArcSplitHarness {
    address internal constant AGGREGATOR = 0xeb12351602c56D47c4EE955193335848952b29d8;
    address internal constant PERMIT2 = 0x000000000022D473030F116dDEE9F6B43aC78BA3;
    address internal constant USDC = 0x3600000000000000000000000000000000000000;
    address internal constant EURC = 0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a;
    address internal constant CIRBTC = 0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF;
    address internal constant PRESTO = 0x5794a8284A29493871Fbfa3c4f343D42001424D6;
    address internal constant XYLO_POOL = 0x3DF3966F5138143dce7a9cFDdC2c0310ce083BB1;

    uint160 internal constant AMOUNT_IN = 21_467_000_000;
    uint256 internal constant PRESTO_AMOUNT_IN = 20_516_011_900;
    uint256 internal constant XYLO_AMOUNT_IN = 950_988_100;

    struct Snapshot {
        uint256 userUsdc;
        uint256 userEurc;
        uint256 aggregatorUsdc;
        uint256 aggregatorEurc;
        uint256 aggregatorCirbtc;
        uint256 xyloReserve0;
        uint256 xyloReserve1;
        uint256 prestoPathReserve;
        uint256 prestoTokenReserve;
    }

    function simulate(uint256 minAmountOut)
        external
        returns (
            bool ok,
            uint256 amountOut,
            bytes memory revertData,
            Snapshot memory beforeState,
            Snapshot memory afterState
        )
    {
        beforeState = _snapshot();

        require(IERC20(USDC).approve(PERMIT2, AMOUNT_IN), "USDC_APPROVE_FAILED");
        IAllowanceTransfer(PERMIT2).approve(USDC, AGGREGATOR, AMOUNT_IN, type(uint48).max);

        Aggregator.SubRoute[] memory routes = _routes();
        Aggregator.Permit2Pull memory pull = Aggregator.Permit2Pull({
            permitSingle: IAllowanceTransfer.PermitSingle({
                details: IAllowanceTransfer.PermitDetails({
                    token: USDC, amount: 0, expiration: 0, nonce: 0
                }),
                spender: AGGREGATOR,
                sigDeadline: type(uint256).max
            }),
            signature: bytes("")
        });

        try Aggregator(payable(AGGREGATOR))
            .splitSwap(
                USDC, EURC, AMOUNT_IN, minAmountOut, type(uint256).max, routes, pull
            ) returns (
            uint256 returnedAmountOut
        ) {
            ok = true;
            amountOut = returnedAmountOut;
        } catch (bytes memory reason) {
            revertData = reason;
        }

        afterState = _snapshot();
    }

    function _routes() private pure returns (Aggregator.SubRoute[] memory routes) {
        routes = new Aggregator.SubRoute[](2);

        Aggregator.Hop[] memory prestoHops = new Aggregator.Hop[](1);
        prestoHops[0] = Aggregator.Hop({
            pool: PRESTO, dexType: Aggregator.DexType.Presto, tokenIn: USDC, tokenOut: EURC, fee: 0
        });
        routes[0] = Aggregator.SubRoute({amountIn: PRESTO_AMOUNT_IN, hops: prestoHops});

        Aggregator.Hop[] memory xyloHops = new Aggregator.Hop[](1);
        xyloHops[0] = Aggregator.Hop({
            pool: XYLO_POOL, dexType: Aggregator.DexType.Xylo, tokenIn: USDC, tokenOut: EURC, fee: 0
        });
        routes[1] = Aggregator.SubRoute({amountIn: XYLO_AMOUNT_IN, hops: xyloHops});
    }

    function _snapshot() private view returns (Snapshot memory state) {
        state.userUsdc = IERC20(USDC).balanceOf(address(this));
        state.userEurc = IERC20(EURC).balanceOf(address(this));
        state.aggregatorUsdc = IERC20(USDC).balanceOf(AGGREGATOR);
        state.aggregatorEurc = IERC20(EURC).balanceOf(AGGREGATOR);
        state.aggregatorCirbtc = IERC20(CIRBTC).balanceOf(AGGREGATOR);
        (state.xyloReserve0, state.xyloReserve1) = IXyloPool(XYLO_POOL).getReserves();
        state.prestoPathReserve = IPrestoReserves(PRESTO).pathReserves(EURC);
        state.prestoTokenReserve = IPrestoReserves(PRESTO).tokenReserves(EURC);
    }
}
