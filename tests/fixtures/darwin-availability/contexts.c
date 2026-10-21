/* The context rule: a use inside an unavailable declaration, record or
 * function is not refused, as in Apple clang 21. */
typedef int gone_t __attribute__((availability(macos,unavailable)));
int gone(void) __attribute__((availability(macos,unavailable)));
int (*p)(void) __attribute__((availability(macos,unavailable))) = gone;
struct __attribute__((availability(macos,unavailable))) S { gone_t m; };
struct T { gone_t m __attribute__((availability(macos,unavailable))); };
struct U { gone_t m; };
static int unused_static(void) { return gone(); }
int later(void) __attribute__((availability(macos,introduced=13.0)));
int later_user(void) __attribute__((availability(macos,introduced=14.0)));
int later_user(void) { return later(); }
int x = sizeof(gone_t);
void param(int a, gone_t b);
gone_t ret(void);
int g(void) { gone_t local = 0; return local; }
int h(void) __attribute__((availability(macos,unavailable)));
int h(void) { gone_t local = 0; return local + gone(); }
