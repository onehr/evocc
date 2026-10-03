/* Input-validation control: all 36 ordered pairs of core-v1 integer types.
 * This is not part of the frozen native or optimization benchmark population.
 */
#define CASE(N, A, B) unsigned long long N(A a, B b) { return a + b; }
#define ROW(N, T) \
    CASE(N##i, T, int) CASE(N##u, T, unsigned int) \
    CASE(N##l, T, long) CASE(N##z, T, unsigned long) \
    CASE(N##q, T, long long) CASE(N##w, T, unsigned long long)
ROW(i, int)
ROW(u, unsigned int)
ROW(l, long)
ROW(z, unsigned long)
ROW(q, long long)
ROW(w, unsigned long long)
