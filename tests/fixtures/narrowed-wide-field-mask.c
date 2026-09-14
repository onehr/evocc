struct field { unsigned long value : 40; };
static int calls;
static struct field storage;
static struct field *next(void) { calls++; return &storage; }
static unsigned low(struct field *p) { return (unsigned)(p->value & 0xffffffff); }
static unsigned reverse(struct field *p) { return (unsigned)(0xffffffff & p->value); }
static unsigned short narrow(struct field *p) { return (unsigned short)(p->value & 0xffff); }
int main(void) {
    storage.value = 0x98abcdef12UL;
    if (low(&storage) != 0xabcdef12U || reverse(&storage) != 0xabcdef12U) return 1;
    if (narrow(&storage) != 0xef12U) return 2;
    if ((unsigned)(next()->value & 0xff) != 0x12U || calls != 1) return 3;
    storage.value = 0xffffffffffUL;
    if (low(&storage) != 0xffffffffU) return 4;
    return 0;
}
