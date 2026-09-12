extern void constant_failed(void) __attribute__((error("constant result is out of range")));
static __attribute__((always_inline)) inline unsigned last_bit(unsigned long word)
{
    return sizeof(word) * 8 - 1 - __builtin_clzl(word);
}
int main(void)
{
    if (sizeof(unsigned char) * 8 <= last_bit(1UL))
        constant_failed();
    return last_bit(128UL) != 7;
}
