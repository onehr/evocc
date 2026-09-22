/* Compile PART=1 and PART=2 independently, then link with PART=3.
   The direct calls execute the inline body; its address names the external
   definition, which deliberately has different observable behaviour. */
#if PART != 3
extern inline __attribute__((gnu_inline, always_inline))
int wrapped(int x, int *side) { ++*side; return x + 100; }
#endif
#if PART == 1
int first(int x, int *side) { return wrapped(x, side); }
int (*address(void))(int, int *) { return wrapped; }
#elif PART == 2
int second(int x, int *side) { return wrapped(x, side); }
#elif PART == 3
int first(int, int *);
int second(int, int *);
int (*address(void))(int, int *);
int wrapped(int x, int *side) { *side += 3; return x + 7; }
int main(void) {
    int side = 0;
    if (first(2, &side) != 102 || second(3, &side) != 103) return 1;
    if (address()(2, &side) != 9 || side != 5) return 2;
    return 0;
}
#endif
