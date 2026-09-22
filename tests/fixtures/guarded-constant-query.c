extern void invalid_signedness(void);
#define nonnegative(x) (__builtin_constant_p((long long)(x) >= 0) && ((long long)(x) >= 0))
int consume(int length)
{
    int total = 0;
    while (length > 0) {
        __auto_type value = length;
        if (!nonnegative(value))
            invalid_signedness();
        unsigned long amount = value < 4096UL ? value : 4096UL;
        total += amount;
        length -= amount;
    }
    return total;
}
int main(void)
{
    return consume(8201) != 8201 || consume(-3) != 0;
}
