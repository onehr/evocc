# Origin

Written for this corpus. This is an original, dependency-free composition of
literal/repeat run-length packets, framed length validation, and a bounded
FNV-style checksum. It is not a claimed upstream defect.

The public functions operate on byte-valued unsigned arrays. The framing entry
points accept payload lengths 0 through 128 and explicit output capacities.
The deterministic `evaluate` driver accepts seeds 0 through 11; its twelve
payloads have different lengths and mixes of literal and repeated bytes.

Two mutations interact in frame construction and verification:

1. Remove the full-width case from `frame_mask`, making the long-frame checksum
   shift a 32-bit value by 32. Short-frame calls use width 16. The entry point
   calls two short payloads before the width-32 payload.
2. Bias the repeat header by one, so the decoder reconstructs an extra byte and
   the framed length/checksum agreement fails even after the UB is repaired.

`fixed.c` restores only those two expressions. The hidden driver checks twelve
distinct seed inputs and calls the original entry point. The checksum and
encoded-frame fingerprint also distinguish the UB-only remaining mutant on
the tested ordinary compiler path; sanitizer evidence is recorded separately.
