struct value { _Complex unsigned short number; };
static struct value stored = { 100 + 200i };
__attribute__((noinline)) static struct value read(void) { return stored; }
int main(void) {
    struct value copy = read();
    if (copy.number != stored.number) return 1;
    _Complex int z = 3 + 4i;
    z *= 2 - 1i;
    return __real__ z != 10 || __imag__ z != 5;
}
