static int evaluations;
static unsigned int cell = 7;
static unsigned int *address(void) { ++evaluations; return &cell; }
static unsigned int operand(void) { ++evaluations; return 3; }

int main(void) {
    unsigned char a = 255;
    short b = 32767;
    unsigned long c = ~0UL;
    if (__sync_fetch_and_add(&a, 2) != 255 || a != 1) return 1;
    if (__sync_add_and_fetch(&b, 1) != -32768) return 2;
    if (__sync_fetch_and_sub(&c, 1) != ~0UL || c != ~1UL) return 3;
    if (__sync_and_and_fetch(&c, 15) != 14) return 4;
    if (__sync_fetch_and_or(&a, 6) != 1 || a != 7) return 5;
    if (__sync_xor_and_fetch(&a, 3) != 4) return 6;
    if (__sync_nand_and_fetch(&a, 7) != 251) return 7;
    if (__sync_fetch_and_nand(&a, 3) != 251 || a != 252) return 8;
    if (__sync_lock_test_and_set(&b, 99) != -32768 || b != 99) return 9;
    __sync_lock_release(&b);
    if (b != 0) return 10;
    if (__sync_fetch_and_add(address(), operand()) != 7 || evaluations != 2 || cell != 10) return 11;
    if (__sync_val_compare_and_swap(&a, 252, 2) != 252 || a != 2) return 12;
    if (__sync_bool_compare_and_swap(&a, 3, 0) || a != 2) return 13;
    int x = 5, y = 9;
    int *p = &x;
    if (__sync_val_compare_and_swap(&p, &x, &y) != &x || *p != 9) return 14;
    if (__sync_lock_test_and_set(&p, &x) != &y || *p != 5) return 15;
    return 0;
}
