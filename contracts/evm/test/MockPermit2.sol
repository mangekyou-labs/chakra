// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {IAllowanceTransfer} from "../src/interfaces/IAllowanceTransfer.sol";
import {IERC20} from "../src/interfaces/IERC20Minimal.sol";

/// @title MockPermit2 — AllowanceTransfer test double.
/// @notice `permit` accepts any 65-byte signature (records + grants the allowance);
///         other lengths revert `InvalidSignature`. `transferFrom` pulls ERC-20
///         using the allowance granted to msg.sender. `approve` simulates an
///         existing allowance so tests can skip the signature entirely.
contract MockPermit2 {
    error InvalidSignature();
    error InsufficientAllowance();
    error ExpiredApproval();
    error ZeroAmount();

    event PermitCalled(
        address indexed owner, address indexed token, uint160 amount, address spender
    );

    /// @notice Test observation port: incremented every time permit() runs.
    uint256 public permitCalls;

    mapping(address => mapping(address => mapping(address => IAllowanceTransfer.Allowance))) public
        allowance;

    /// @notice Simulate a pre-existing AllowanceTransfer allowance (no signature needed).
    function approve(
        address user,
        address token,
        address spender,
        uint160 amount,
        uint48 expiration
    ) external {
        IAllowanceTransfer.Allowance storage a = allowance[user][token][spender];
        a.amount = amount;
        a.expiration = expiration;
        a.nonce += 1;
    }

    /// @notice Grant an allowance from a signed PermitSingle. Rejects malformed signatures.
    function permit(
        address owner,
        IAllowanceTransfer.PermitSingle calldata permitSingle,
        bytes calldata signature
    ) external {
        if (signature.length != 65) {
            revert InvalidSignature();
        }
        IAllowanceTransfer.Allowance storage a =
            allowance[owner][permitSingle.details.token][permitSingle.spender];
        a.amount = permitSingle.details.amount;
        a.expiration = permitSingle.details.expiration;
        a.nonce += 1;
        permitCalls += 1;
        emit PermitCalled(
            owner, permitSingle.details.token, permitSingle.details.amount, permitSingle.spender
        );
    }

    /// @notice Pull ERC-20 `amount` of `token` from `from` using msg.sender's allowance.
    function transferFrom(address from, address to, uint160 amount, address token) external {
        IAllowanceTransfer.Allowance storage a = allowance[from][token][msg.sender];
        if (a.amount < amount) revert InsufficientAllowance();
        if (a.expiration < uint48(block.timestamp)) revert ExpiredApproval();
        if (amount == 0) revert ZeroAmount();
        a.amount -= amount;
        IERC20(token).transferFrom(from, to, amount);
    }
}
