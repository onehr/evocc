unsigned char byte;
unsigned short half;
unsigned word;
unsigned long wide;
int object = 23;
int *pointer;
int address_calls, value_calls, order_calls;
typedef int (*callback)(int);
callback callback_slot;
int increment(int n) { return n + 1; }

__attribute__((noinline)) unsigned *address(void) { ++address_calls; return &word; }
__attribute__((noinline)) unsigned value(void) { ++value_calls; return 29; }
__attribute__((noinline)) int order(int n) { ++order_calls; return n; }

unsigned char load_byte(unsigned char *p) { return __atomic_load_n(p, 0); }
unsigned short load_half(unsigned short *p) { return __atomic_load_n(p, 2); }
unsigned load_word(unsigned *p) { return __atomic_load_n(p, 5); }
unsigned long load_wide(unsigned long *p) { return __atomic_load_n(p, 1); }
void store_byte(unsigned char *p, unsigned char v) { __atomic_store_n(p, v, 0); }
void store_half(unsigned short *p, unsigned short v) { __atomic_store_n(p, v, 3); }
void store_word(unsigned *p, unsigned v) { __atomic_store_n(p, v, 5); }
void store_wide(unsigned long *p, unsigned long v) { __atomic_store_n(p, v, 3); }

int main(void) {
    store_byte(&byte, 251);
    store_half(&half, 65530);
    store_word(&word, 0xfffffff1u);
    store_wide(&wide, 0xffffffff00000001ul);
    if (load_byte(&byte) != 251 || load_half(&half) != 65530) return 1;
    if (load_word(&word) != 0xfffffff1u) return 2;
    if (load_wide(&wide) != 0xffffffff00000001ul) return 3;
    __atomic_store_n(&pointer, &object, 3);
    if (__atomic_load_n(&pointer, 2) != &object) return 4;
    if (*__atomic_load_n(&pointer, 5) != 23) return 5;
    __atomic_store_n(address(), value(), order(3));
    if (__atomic_load_n(address(), order(2)) != 29) return 6;
    if (address_calls != 2 || value_calls != 1 || order_calls != 2) return 7;
    volatile int v = -17;
    if (__atomic_load_n(&v, 0) != -17) return 8;
    __atomic_store_n(&v, -31, 5);
    if (__atomic_load_n((const volatile int *)&v, 1) != -31) return 9;
    __atomic_store_n(&pointer, (void *)0, 0);
    if (__atomic_load_n(&pointer, 0) != (void *)0) return 10;
    __atomic_store_n(&callback_slot, increment, 3);
    if (__atomic_load_n(&callback_slot, 2)(41) != 42) return 11;
    signed char narrow = -100;
    __atomic_store_n(&narrow, -99, 0);
    if (__atomic_load_n(&narrow, 0) != -99) return 12;
    _Bool boolean = 0;
    __atomic_store_n(&boolean, 7, 0);
    if (__atomic_load_n(&boolean, 0) != 1) return 13;
    return 0;
}
