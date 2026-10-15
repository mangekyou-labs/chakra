# Chakra

Chakra routes swaps across eligible Arc liquidity pools.

## Language

**Successful-poll progress**:
The chain head, confirmed head, indexed head, and time observed at the end of one fully successful analytics poll.
_Avoid_: Latest swap time

**Page cursor**:
The last block whose analytics page was fully indexed. It may advance before a whole poll succeeds.

**Live pool topology**:
The current set of verified pools available to the worker, identified by venue source and pool address.

**Confirmed swap**:
A submitted swap whose receipt reports successful execution after one confirmation.
_Avoid_: Submitted swap
