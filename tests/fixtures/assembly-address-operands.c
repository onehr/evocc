static int values[2] = {17, 29};
static int calls;
static int *address(void) { ++calls; return values; }

int main(void) {
    int first, second, third;
    int *cursor = values;
    asm volatile("ldr %w0, %a1" : "=r"(first) : "p"(cursor++));
    asm volatile("ldr %w[result], %a[addr]"
                 : [result] "=r"(second) : [addr] "r"(cursor));
    asm volatile("ldr %w0, %a1" : "=r"(third) : "p"((unsigned long)address()));
    register int *bound asm("x8") = values;
    asm volatile("prfm pldl1keep, %a[addr]" : : [addr] "p"(bound));
    if (first != 17 || second != 29 || third != 17 || calls != 1 || cursor != values + 1)
        return 1;
    asm goto("ldr w9, %a0\n\tcbnz w9, %l[nonzero]" : : "p"(values) : "x9" : nonzero);
    return 2;
nonzero:
    return 0;
}
