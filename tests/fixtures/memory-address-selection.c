struct record { unsigned long first, second, third; };

__attribute__((noinline))
unsigned long read_third(volatile struct record *p)
{
    return p->third;
}

__attribute__((noinline))
void write_second(volatile struct record *p, unsigned long value)
{
    p->second = value;
}

struct __attribute__((packed)) packed_record { unsigned char tag; unsigned long value; };

__attribute__((noinline))
unsigned long packed_roundtrip(volatile struct packed_record *p, unsigned long value)
{
    p->value = value;
    return p->value;
}

__attribute__((noinline))
unsigned long previous(volatile unsigned long *p)
{
    return p[-1];
}

int main(void)
{
    unsigned long values[] = {0, 1, 0x123456789abcdef0UL, ~0UL};
    for (unsigned i = 0; i < 4; ++i) {
        struct record r = {11, 22, values[i]};
        struct packed_record p = {7, 0};
        write_second(&r, values[i]);
        if (r.first != 11 || r.second != values[i] || read_third(&r) != values[i])
            return 1;
        if (packed_roundtrip(&p, values[i]) != values[i] || p.tag != 7)
            return 2;
        if (previous(values + i + 1) != values[i])
            return 3;
    }
    return 0;
}
