# Rational resampler

Written for this corpus. No external source body was copied.
This is integer linear interpolation with a reduced rational position cursor,
Euclidean division and bounded signal buffers. The comments define its inputs.

Two minimal mutations remove the pre-multiply widening and reverse the sign
test for floor division's remainder correction. They interact in interpolation:
one changes ordinary descending segments, while the other overflows only when
stream 11 crosses the large negative-to-positive transition at fractional
positions. Integer positions and the other streams call the same helpers.

The correct difference magnitude is at most 200000000, and the fractional
product fits signed 64 bits. Output is bounded to the supplied capacity.
The hidden driver uses twelve genuinely different signals and the original
main. Each partial repair must still fail hidden scoring before qualification.
