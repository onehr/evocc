/* Oracle-only FP contract probe, not an EvoCC execution acceptance test. */
#include <float.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

static uint64_t fpcr(void) {
    uint64_t value;
    __asm__ volatile ("mrs %0, fpcr" : "=r"(value));
    return value;
}
static void set_fpcr(uint64_t value) {
    __asm__ volatile ("msr fpcr, %0; isb" : : "r"(value) : "memory");
}
static double from_bits(uint64_t bits) {
    double value;
    memcpy(&value, &bits, 8);
    return value;
}
static uint64_t bits(double value) {
    uint64_t result;
    memcpy(&result, &value, 8);
    return result;
}
__attribute__((noinline))
static double add(double a, double b) { return a + b; }

static void pair(const char *name, uint64_t a_bits, uint64_t b_bits) {
    double a = from_bits(a_bits), b = from_bits(b_bits);
    double sum, product, quotient, negative;
    /* Pin instruction operand order independently of C optimization. */
    __asm__ volatile ("fadd %d0, %d1, %d2" : "=w"(sum) : "w"(a), "w"(b));
    __asm__ volatile ("fmul %d0, %d1, %d2" : "=w"(product) : "w"(a), "w"(b));
    __asm__ volatile ("fdiv %d0, %d1, %d2" : "=w"(quotient) : "w"(a), "w"(b));
    __asm__ volatile ("fneg %d0, %d1" : "=w"(negative) : "w"(a));
    printf("%s add=%016lx mul=%016lx div=%016lx neg=%016lx\n",
           name, bits(sum), bits(product), bits(quotient), bits(negative));
}
int main(void) {
    uint64_t original = fpcr();
    printf("initial-fpcr=%016lx eval=%d radix=%d float=%zu double=%zu long-double=%zu\n",
           original, FLT_EVAL_METHOD, FLT_RADIX,
           sizeof(float), sizeof(double), sizeof(long double));
    set_fpcr(0);
    pair("q1-q2", 0x7ff8000000000011UL, 0xfff8000000000022UL);
    pair("q2-q1", 0xfff8000000000022UL, 0x7ff8000000000011UL);
    pair("q1-s2", 0x7ff8000000000011UL, 0xfff0000000000022UL);
    pair("s1-q2", 0x7ff0000000000011UL, 0xfff8000000000022UL);
    pair("s1-s2", 0x7ff0000000000011UL, 0xfff0000000000022UL);
    pair("zero-zero", 0, 0);
    pair("smallest-two", 1, 0x4000000000000000UL);
    volatile double one = 1.0, half = 0x1p-53;
    printf("round-nearest=%016lx\n", bits(add(one, half)));
    set_fpcr(1UL << 22);
    printf("round-upward=%016lx\n", bits(add(one, half)));
    set_fpcr(1UL << 24);
    pair("flush-smallest-two", 1, 0x4000000000000000UL);
    set_fpcr(1UL << 25);
    pair("default-nan", 0x7ff8000000000011UL, 0xfff8000000000022UL);
    set_fpcr(original);
    return 0;
}
