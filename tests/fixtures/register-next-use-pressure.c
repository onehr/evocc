/* Repeated reads with different next-use and last-use order. Unsigned
 * arithmetic is defined; volatile inputs prevent constant replacement.
 */
static volatile unsigned input[27];

__attribute__((noinline)) static unsigned pressured(void)
{
#define READ(n) unsigned x##n = input[n]
    READ(0); READ(1); READ(2); READ(3); READ(4); READ(5); READ(6);
    READ(7); READ(8); READ(9); READ(10); READ(11); READ(12); READ(13);
    READ(14); READ(15); READ(16); READ(17); READ(18); READ(19); READ(20);
    READ(21); READ(22); READ(23); READ(24); READ(25); READ(26);
#undef READ
    unsigned result = x0 + x26;
#define ADD(n) result += x##n
    ADD(2); ADD(3); ADD(4); ADD(5); ADD(6); ADD(7); ADD(8); ADD(9);
    ADD(10); ADD(11); ADD(12); ADD(13); ADD(14); ADD(15); ADD(16);
    ADD(17); ADD(18); ADD(19); ADD(20); ADD(21); ADD(22); ADD(23);
    ADD(24); ADD(25);
#undef ADD
    return result + x1 + x0;
}

int main(void)
{
    for (unsigned round = 0; round < 16; ++round) {
        unsigned expected = 0;
        for (unsigned i = 0; i < 27; ++i) {
            unsigned value = (round * 7919 + i) ^ 0xf0f0f0f0U;
            input[i] = value;
            expected += value;
        }
        expected += input[0];
        if (pressured() != expected)
            return 1;
    }
    return 0;
}
