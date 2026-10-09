# Source

Written for this corpus. No upstream code is claimed.
Left shift returns zero when the unsigned count is at least 32.
One E3005 defect is injected into transform. The hidden test covers twelve
input seeds, each with 32 samples. main executes the same transform and
returns the processed count. The expected checksum uses unsigned 64-bit
FNV-style folding after the stated integer transform.
