// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @notice XyloNet stableswap venue (Arc testnet). Different ABI from the
///      Chakra stableswap: `swap` pulls via `transferFrom`, the factory
///      membership check is `getPool(address,address)` (not Uni V2
///      `getPair`, not the Chakra stable factory), and the fee is taken on
///      output. T-XYLO: scoped to the catalog USDC/EURC pool only.
interface IXyloFactory {
    function getPool(address tokenA, address tokenB) external view returns (address pool);
}

interface IXyloPool {
    function token0() external view returns (address);

    function token1() external view returns (address);

    function getReserves() external view returns (uint256 reserve0, uint256 reserve1);

    /// @dev Pulls `amountIn` from the caller via `transferFrom`.
    function swap(
        address tokenIn,
        address tokenOut,
        uint256 amountIn,
        uint256 minOut,
        address to,
        uint256 deadline
    ) external returns (uint256 amountOut);
}
