# Fixed-point ledger

Written for this corpus. No external implementation was copied.
The program composes integer monetary rounding, bounded account updates and
transaction rollback. It has four accounts and atomic batches of postings.

The two injected defects are multiplication before widening, and rounding
negative fees toward the wrong half-unit boundary. They share the fee path.
The large positive posting in stream 11 exceeds int multiplication range;
ordinary postings use the same function safely. Negative fees are rebates,
so the rounding defect changes both balances and cumulative fees.

The comments define all numeric limits. Correct products and balances fit
signed 64 bits, and input deltas exclude INT_MIN. Every unsigned evaluate seed
is reduced modulo twelve. Hidden tests use twelve distinct posting streams
and the original main, including failed batch rollback. Each single-repair
control must fail; changing only the large-input path is insufficient.
