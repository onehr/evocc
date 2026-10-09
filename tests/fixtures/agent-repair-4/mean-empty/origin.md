# Source

Written for this corpus. No upstream code is claimed.
The mean of an empty bucket is -1. Other means use truncation toward zero.
One E3002 defect is injected into transform. The hidden test covers twelve
input seeds, each with 32 samples. main executes the same transform and
returns the processed count. The expected checksum uses unsigned 64-bit
FNV-style folding after the stated integer transform.

Qualification replacement 1: the first candidate specified zero for an empty
bucket. All three non-sanitized AArch64 builds happened to produce that value
for the original division by zero. That candidate failed the hidden-failure
criterion. This candidate specifies the conventional error sentinel -1 in
the source contract. It is qualified before any agent session.
