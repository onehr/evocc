/* Nonzero depths are runtime null values, not C null pointer constants.
 * Depth conversion is unsigned int on both targets. No ancestor is read. */
int object;
_Static_assert(__builtin_types_compatible_p(
    __typeof__(1 ? &object : __builtin_return_address(1)), void *), "return type");
_Static_assert(__builtin_types_compatible_p(
    __typeof__(1 ? &object : __builtin_frame_address(1)), void *), "frame type");

__attribute__((noinline)) int probe(void)
{
    if (__builtin_return_address(1) || __builtin_frame_address(1)) return 1;
    if (__builtin_return_address(2) || __builtin_frame_address(2)) return 2;
    if (__builtin_return_address(65535) || __builtin_frame_address(65535)) return 3;
    if (__builtin_return_address(0x100000001UL) ||
        __builtin_frame_address(-4294967295L)) return 4;
    if (!__builtin_return_address(0x100000000UL) ||
        !__builtin_frame_address(0x100000000UL)) return 5;
    if (__builtin_extract_return_addr(__builtin_return_address(1))) return 6;
    return 0;
}
int main(void) { return probe(); }
