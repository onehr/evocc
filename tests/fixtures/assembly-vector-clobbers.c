__attribute__((noinline)) void overwrite_vectors(void)
{
    asm volatile(".arch_extension simd\n\tmovi v0.16b, #0\n\t"
                 "movi v8.16b, #0\n\t"
                 "movi v15.16b, #0\n\t"
                 "movi v31.16b, #0"
                 : : : "v0", "v8", "v15", "v31");
}

int main(void)
{
    unsigned long first = 0x123456789abcdef0UL;
    unsigned long last = 0xfedcba9876543210UL;
    unsigned long actual_first, actual_last;
    asm volatile(".arch_extension simd\n\tfmov d8, %x0\n\tfmov d15, %x1"
                 : : "r"(first), "r"(last) : "v8", "v15");
    overwrite_vectors();
    asm volatile(".arch_extension simd\n\tfmov %x0, d8\n\tfmov %x1, d15"
                 : "=r"(actual_first), "=r"(actual_last));
    return actual_first != first || actual_last != last;
}
