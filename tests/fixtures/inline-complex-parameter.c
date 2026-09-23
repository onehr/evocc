/* An inlined integer-complex parameter uses the ordinary pair conversion,
 * not generic scalar Cast. Its result storage outlives the inlined body. */
typedef _Complex short pair;
static inline __attribute__((always_inline)) pair twice(pair x) { return x + x; }
int main(void) {
    pair p = 3 + 4i;
    pair q = twice(p);
    return __real__ q != 6 || __imag__ q != 8;
}
