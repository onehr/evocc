struct __attribute__((packed)) object {
    unsigned char tag;
    unsigned long value;
    void *pointer;
    unsigned __int128 wide;
};
__attribute__((noinline)) unsigned long read_value(struct object *p) { return p->value; }
__attribute__((noinline)) void write_value(struct object *p, unsigned long n) { p->value=n; }
__attribute__((noinline)) void *read_pointer(struct object *p) { return p->pointer; }
__attribute__((noinline)) void write_pointer(struct object *p, void *n) { p->pointer=n; }
__attribute__((noinline)) unsigned __int128 read_wide(struct object *p) { return p->wide; }
__attribute__((noinline)) void write_wide(struct object *p, unsigned __int128 n) { p->wide=n; }
int main(void) {
    struct object p;
    int n=23;
    write_value(&p, 0xfedcba9876543210UL);
    write_pointer(&p, &n);
    unsigned __int128 wide=((unsigned __int128)0x1234567890abcdefUL << 64) | 0xfedcba9876543210UL;
    write_wide(&p, wide);
    return read_value(&p) != 0xfedcba9876543210UL || read_pointer(&p) != &n || read_wide(&p) != wide;
}
