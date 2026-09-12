typedef union { unsigned word; unsigned short halves[2]; } ticket;
typedef union { unsigned long word; unsigned halves[2]; } wide;
int main(void)
{
    ticket a;
    asm volatile("mov %w0, #0x1234" : "=r"(a));
    asm volatile("add %w0, %w0, #1" : "+r"(a));
    if (a.word != 0x1235 || a.halves[0] != 0x1235 || a.halves[1]) return 1;
    wide b;
    asm volatile("mov %0, #1\n\tlsl %0, %0, #40" : "=&r"(b));
    asm volatile("add %0, %0, #7" : "+r"(b));
    if (b.word != (1UL << 40) + 7) return 2;
    unsigned raw;
    asm volatile("mov %w0, %w1" : "=r"(raw) : "r"(a));
    return raw != 0x1235;
}
