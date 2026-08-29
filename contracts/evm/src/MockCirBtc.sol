// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

/// @title Mock cirBTC — 8 decimal ERC-20 test double for the canonical
///      catalog token (real cirBTC is `0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF`).
/// @notice Owner mint only. Used by chain-31337 Foundry fixtures; the Arc
///      operator workflow never deploys it (canonical cirBTC already exists).
contract MockCirBtc {
    error NotOwner();
    error InsufficientBalance();
    error ZeroAddress();

    string public constant name = "Circle BTC";
    string public constant symbol = "cirBTC";
    uint8 public constant decimals = 8;

    address public immutable owner;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    function mint(address to, uint256 amount) external onlyOwner {
        if (to == address(0)) revert ZeroAddress();
        totalSupply += amount;
        balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 allowed = allowance[from][msg.sender];
        if (allowed != type(uint256).max) {
            allowance[from][msg.sender] = allowed - amount;
        }
        _transfer(from, to, amount);
        return true;
    }

    function _transfer(address from, address to, uint256 amount) internal {
        if (to == address(0)) revert ZeroAddress();
        uint256 bal = balanceOf[from];
        if (bal < amount) revert InsufficientBalance();
        unchecked {
            balanceOf[from] = bal - amount;
        }
        balanceOf[to] += amount;
        emit Transfer(from, to, amount);
    }
}
