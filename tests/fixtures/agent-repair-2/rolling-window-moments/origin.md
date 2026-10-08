# Rolling window moments

Written for this corpus. No external source body was copied.
The algorithms are a circular buffer and exact integer first and second moments.

The generic window accepts capacity 1..16 and samples -100000..100000.
All sums and variance products fit signed 64 bits. `evaluate` accepts every
unsigned seed and reduces it modulo 12 before generating its stream.

Two interacting mutations change the sample-moment path: square a sample in
signed int before widening, and subtract the next slot instead of the slot
being overwritten when the ring is full. Only the large sample in stream 11
overflows int. Other calls to the same square helper use small samples.
The correct repair widens before multiplication and evicts the current slot.
Neither repair changes the public API or input domain.

The hidden driver exercises twelve streams and the original main. The first
defect affects large-sample moments; the second affects every wrapped window.
Each one-repair control must still fail before this item can be qualified.
