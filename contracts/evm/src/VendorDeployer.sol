// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.30;

import {Script} from "forge-std/Script.sol";

/// @title VendorDeployer — deploy pre-compiled V2/V3 from hex bytecodes.
/// @dev Bytecodes compiled offline with solc 0.5.16 / 0.7.6, stored as hex
///      files under `bytecodes/`. Inherit this from Test or Script to bootstrap
///      V2/V3 factories without cross-version imports.
abstract contract VendorDeployer is Script {
    /// @notice Deploy from a hex file, optionally appending abi-encoded args.
    function _deployFromHexFile(string memory hexFile) internal returns (address addr) {
        return _deployFromHexFileWithArgs(hexFile, "");
    }

    function _deployFromHexFileWithArgs(string memory hexFile, bytes memory args)
        internal
        returns (address addr)
    {
        string memory hexCode = vm.readFile(hexFile);
        bytes memory code = _hexToBytes(hexCode);
        bytes memory initCode;
        if (args.length > 0) {
            initCode = abi.encodePacked(code, args);
        } else {
            initCode = code;
        }
        assembly {
            addr := create(0, add(initCode, 0x20), mload(initCode))
        }
        require(addr != address(0), "VendorDeployer: deployment failed");
    }

    /// @dev Convert hex string to bytes (ignores whitespace).
    function _hexToBytes(string memory hex_) internal pure returns (bytes memory) {
        bytes memory raw = bytes(hex_);
        uint256 len = raw.length;

        uint256 count = 0;
        for (uint256 i = 0; i < len; i++) {
            if (_isHexChar(raw[i])) count++;
        }
        require(count % 2 == 0, "VendorDeployer: odd hex digits");
        uint256 bytesLen = count / 2;

        bytes memory result = new bytes(bytesLen);
        uint256 nibbleIdx = 0;
        for (uint256 i = 0; i < len; i++) {
            bytes1 c = raw[i];
            if (!_isHexChar(c)) continue;
            uint8 val = _hexVal(c);
            if (nibbleIdx % 2 == 0) {
                result[nibbleIdx / 2] = bytes1(val << 4);
            } else {
                result[nibbleIdx / 2] = result[nibbleIdx / 2] | bytes1(val);
            }
            nibbleIdx++;
        }
        return result;
    }

    function _isHexChar(bytes1 c) internal pure returns (bool) {
        return (c >= 0x30 && c <= 0x39) || (c >= 0x41 && c <= 0x46) || (c >= 0x61 && c <= 0x66);
    }

    function _hexVal(bytes1 c) internal pure returns (uint8) {
        if (c >= 0x30 && c <= 0x39) return uint8(c) - 0x30;
        if (c >= 0x41 && c <= 0x46) return uint8(c) - 0x41 + 10;
        if (c >= 0x61 && c <= 0x66) return uint8(c) - 0x61 + 10;
        revert("VendorDeployer: not hex");
    }
}
