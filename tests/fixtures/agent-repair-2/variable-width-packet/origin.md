# Origin

Written for this corpus. This original bit-packet implementation composes
cross-word field writing/reading, per-record width metadata, and an optional
full-width sequence extension. It is not a claimed upstream defect.

Packets contain 0 through 16 records. Each unsigned record must fit its stated
width, from 1 through 32. Storage capacities are explicit and limited to 64
32-bit words. `evaluate` accepts seeds 0 through 11 and constructs twelve
different width/value sequences, including compact and extended packets.

Two mutations interact in reconstructing the same bit stream:

1. Remove the width-32 case from `packet_mask`. Compact fields take ordinary
   widths; the aligned sequence extension reaches the invalid shift of 32.
   The entry point calls two compact packets before the extended packet.
2. Advance a cross-word read's destination shift by the remaining bit count
   instead of the number just consumed. This corrupts records crossing word
   boundaries, even when the full-width mask is corrected.

`fixed.c` restores only these expressions. The hidden driver checks twelve
distinct seed inputs and the original entry point. Decoded widths, values,
sequence metadata, and a signature are checked, so a valid-looking packet
with either mutation remaining cannot substitute for the intended round trip.
