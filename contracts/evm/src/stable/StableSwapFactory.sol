// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {StableSwap} from "./StableSwap.sol";

/// @title StableSwapFactory — deploy 2-token StableSwap pools.
contract StableSwapFactory {
    error PoolExists();

    event PoolCreated(address indexed tokenA, address indexed tokenB, address pool);

    /// @notice tokenA -> pool address. Both orderings map to the same pool.
    mapping(address => mapping(address => address)) public getPool;

    /// @notice Deploy a new StableSwap pool for (tokenA, tokenB).
    /// @return pool Address of the new pool.
    function createPool(address tokenA, address tokenB) external returns (address pool) {
        if (getPool[tokenA][tokenB] != address(0)) revert PoolExists();

        // Sort tokens for canonical ordering
        (address t0, address t1) = tokenA < tokenB ? (tokenA, tokenB) : (tokenB, tokenA);

        pool = address(new StableSwap(t0, t1, address(this)));

        getPool[t0][t1] = pool;
        getPool[t1][t0] = pool;

        emit PoolCreated(tokenA, tokenB, pool);
    }
}
