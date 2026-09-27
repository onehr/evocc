/* Disabling type-based alias assumptions must not reuse a stale load. */
__attribute__((noinline))
unsigned overwrite32(unsigned *integer, float *floating) {
    *integer = 0;
    *floating = 1.0f;
    return *integer;
}

__attribute__((noinline))
unsigned long overwrite64(unsigned long *integer, double *floating) {
    *integer = 0;
    *floating = -2.0;
    return *integer;
}

int main(void) {
    union { unsigned i; float f; } a;
    union { unsigned long i; double d; } b;
    return overwrite32(&a.i, &a.f) != 0x3f800000U ||
           overwrite64(&b.i, &b.d) != 0xc000000000000000UL;
}
