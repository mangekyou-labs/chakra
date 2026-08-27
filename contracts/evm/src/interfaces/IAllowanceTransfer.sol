// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @notice Permit2 AllowanceTransfer ABI (Uniswap), selectors only.
/// @dev Matching the real Permit2 predeploy: structs + permit/transferFrom/allowance.
interface IAllowanceTransfer {
    struct PermitDetails {
        address token;
        uint160 amount;
        uint48 expiration;
        uint48 nonce;
    }

    struct PermitSingle {
        PermitDetails details;
        address spender;
        uint256 sigDeadline;
    }

    struct Allowance {
        uint160 amount;
        uint48 expiration;
        uint48 nonce;
    }

    /// @notice Grant (or renew) an exact-amount allowance from a signed PermitSingle.
    function permit(address owner, PermitSingle calldata permitSingle, bytes calldata signature)
        external;

    /// @notice Pull `amount` of `token` from `from` into `to` using msg.sender's allowance.
    function transferFrom(address from, address to, uint160 amount, address token) external;

    /// @notice Current AllowanceTransfer allowance for (user, token, spender).
    function allowance(address user, address token, address spender)
        external
        view
        returns (Allowance memory);
}
