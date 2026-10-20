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

__attribute__((noinline))
void update_second(volatile struct record *p, unsigned long value)
{
    p->second ^= value;
}

__attribute__((noinline))
void update_previous(volatile unsigned long *p, unsigned long value)
{
    p[-1] ^= value;
}

__attribute__((noinline))
void update_packed(volatile struct packed_record *p, unsigned long value)
{
    p->value ^= value;
}

__attribute__((noinline))
unsigned long effect(volatile unsigned long *p)
{
    return ++*p;
}

__attribute__((noinline))
void delayed_store(volatile struct record *p, volatile unsigned long *counter)
{
    volatile unsigned long *q = &p->second;
    unsigned long value = effect(counter);
    *q = value;
}

__attribute__((noinline))
unsigned long delayed_load(volatile struct record *p, volatile unsigned long *counter)
{
    volatile unsigned long *q = &p->second;
    unsigned long value = *counter;
    return *q ^ value;
}

__attribute__((noinline))
void guarded_update(volatile struct record *p, unsigned long divisor)
{
    p->second /= divisor;
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
        update_second(&r, ~0UL);
        if (r.first != 11 || r.second != ~values[i] || r.third != values[i])
            return 4;
        unsigned long pair[2] = {values[i], 0};
        update_previous(pair + 1, ~0UL);
        if (pair[0] != ~values[i] || pair[1] != 0)
            return 5;
        update_packed(&p, ~0UL);
        if (p.value != ~values[i] || p.tag != 7)
            return 6;
        unsigned long counter = 40;
        delayed_store(&r, &counter);
        if (r.second != 41 || counter != 41)
            return 7;
        if (delayed_load(&r, &counter) != 0)
            return 8;
        guarded_update(&r, 41);
        if (r.second != 1)
            return 9;
    }
    return 0;
}
