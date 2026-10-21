/* Records: a member takes the context of its record and then of the
 * enclosing function, as in Apple clang 21. */
typedef int gone_t __attribute__((availability(macos,unavailable)));
int gone(void) __attribute__((availability(macos,unavailable)));
void use1(void) { int y __attribute__((unavailable)) = gone(); }
void use2(void) { int lf(gone_t) __attribute__((unavailable)); }
struct __attribute__((unavailable)) Outer { struct Inner { gone_t m; } in; };
struct M { gone_t m1 __attribute__((unavailable)), m2; };
struct N { gone_t n1, n2 __attribute__((unavailable)); };
void use3(void) __attribute__((unavailable));
void use3(void) { struct L { gone_t q; } l; (void)l; }
int arr[sizeof(gone_t)] __attribute__((unavailable));
void (*fp)(gone_t) __attribute__((unavailable));
