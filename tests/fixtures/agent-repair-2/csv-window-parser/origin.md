# CSV quantity parser

Written for this corpus. No external source body was copied.
This is a bounded CSV reader composed with decimal formatting and a saturating
quantity parser. Quoted labels, doubled quotes and grouped decimal quantities
have the behavior stated in the source comments.

Two mutations affect the same quantity parser: advance two bytes at an
underscore instead of one, and omit the last digit from the overflow threshold.
Ordinary identifier and quantity calls stay within signed int range. The final
quantity in stream 11 reaches the incorrect saturation boundary and overflows.
Fixing either mutation alone leaves wrong aggregate results on hidden inputs.

All twelve hidden seed values produce distinct identifiers and quantities;
the original main is also called. Every unsigned evaluate seed is accepted
through modulo reduction. Temporary buffers have explicit fixed bounds.
