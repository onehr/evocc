/* L11b: compile-only operand and order controls, NEG=0..10. Compile each
 * with -c and record whether the compiler accepts, warns or rejects. */
#ifndef NEG
#define NEG 0
#endif
struct record { int a, b; };
_Bool flag;
float real;
struct record object;
const int fixed = 1;
void *opaque;
int value, expected;

int probe(void)
{
#if NEG == 0
    return __atomic_fetch_add(&flag, 1, 5);
#elif NEG == 1
    return __atomic_fetch_add(&real, 1.0f, 5);
#elif NEG == 2
    return __atomic_load_n(&object, 5).a;
#elif NEG == 3
    return __atomic_fetch_add(&fixed, 1, 5);
#elif NEG == 4
    return __atomic_compare_exchange_n(&value, &expected, 1, 0, 5, 3);
#elif NEG == 5
    return __atomic_compare_exchange_n(&value, &expected, 1, 0, 5, 4);
#elif NEG == 6
    __atomic_clear(&flag, 2); return 0;
#elif NEG == 7
    __atomic_store_n(&value, 1, 2); return 0;
#elif NEG == 8
    return __atomic_fetch_add(&opaque, 1, 5) != 0;
#elif NEG == 9
    return __atomic_fetch_add(&value, 1, 6);
#elif NEG == 10
    return __atomic_compare_exchange_n(&value, &expected, 1, 0, 0, 5);
#else
#error unsupported NEG
#endif
}
