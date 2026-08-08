# Safety

The caller must uphold the corresponding C declaration and the function's
documented contract:

- every pointer the function accesses must be valid and suitably aligned for
  that access, writable where required, and live for the documented duration;
- opaque handles must originate from the matching constructor;
- strings and arrays must have their documented bounds or terminators;
- callbacks and variadic arguments must have the declared ABI and type;
- calls that touch process-global state must be externally serialized; and
- every documented ownership transfer must be honored.
