# Origin

Written for this corpus. This original implementation composes canonical
prefix-code assignment, oversubscription checks, bounded word-stream encoding,
and range-based decoding. It is not a claimed upstream defect.

Books have one through eight symbols and lengths 0 through 8, where zero marks
an absent symbol. At least one symbol must be present, and the code lengths
must be prefix-free. Encoders accept 0 through 160 symbols and explicit word
capacities. `evaluate` accepts seeds 0 through 11 and constructs twelve
distinct inputs using three complete codebook families.

Two mutations interact at the encoded-word/decoder boundary:

1. Remove the full-width case from `prefix_mask`. Codebook calls use widths
   1 through 8, and partial final words use widths below 32. A stream ending
   exactly at a word boundary reaches the invalid width-32 shift while
   normalizing padding, corrupting that final word on the tested ordinary path.
2. Exclude the first canonical code of each length from the decoder's range
   test. A correctly preserved word is still decoded incorrectly whenever
   it contains a range's first symbol.

`fixed.c` restores only these expressions. The entry point exercises two
partial-word streams before a whole-word stream. The hidden driver checks
twelve distinct seed inputs and that original entry point. Every decoded
symbol is compared with the generated input before its signature is returned.
