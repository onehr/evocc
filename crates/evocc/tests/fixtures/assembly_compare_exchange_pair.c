typedef unsigned __int128 pair;
union halves { pair full; struct { unsigned long low, high; }; };

/* The memory address also appears as a fixed input. The expected values
 * appear both as tied outputs and as independent, unfixed inputs. */
pair compare_exchange(volatile pair *pointer, pair old, pair value) {
    union halves result, expected = { .full = old }, replacement = { .full = value };
    register unsigned long x0 asm("x0") = expected.low;
    register unsigned long x1 asm("x1") = expected.high;
    register unsigned long x2 asm("x2") = replacement.low;
    register unsigned long x3 asm("x3") = replacement.high;
    register unsigned long x4 asm("x4") = (unsigned long)pointer;
    asm volatile(".arch_extension lse\ncasp %[old1], %[old2], %[new1], %[new2], %[memory]"
        : [old1] "+&r" (x0), [old2] "+&r" (x1), [memory] "+Q" (*pointer)
        : [new1] "r" (x2), [new2] "r" (x3), [address] "r" (x4),
          [old_low] "r" (expected.low), [old_high] "r" (expected.high));
    result.low = x0;
    result.high = x1;
    return result.full;
}
