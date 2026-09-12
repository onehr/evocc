extern void constant_failed(void) __attribute__((error("constant branch was not folded")));
int main(void) {
    static const unsigned long end = -(1UL << 47);
    if (end != -(1UL << 47)) constant_failed();
    if ( (((end - 1) >> 48) & 15) == ((end >> 48) & 15)
         && (((-(1UL << 48)) >> 48) & 15) != 15) constant_failed();
    return 0;
}
