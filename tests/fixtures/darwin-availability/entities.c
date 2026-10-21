/* Unavailable typedefs, tags, enumerators and members, and the uses that
 * Apple clang 21 refuses. */
typedef int gone_t __attribute__((availability(macos,unavailable)));
struct __attribute__((availability(macos,unavailable))) gone_s { int x; };
enum { E_OK, E_GONE __attribute__((availability(macos,unavailable))) };
struct holder { int a; int gone_member __attribute__((availability(macos,unavailable))); };
gone_t v1;
struct gone_s v2;
int f1(void) { return E_GONE; }
int f2(struct holder *h) { return h->gone_member; }
gone_t f3(void) __attribute__((availability(macos,unavailable)));
typedef gone_t also_gone __attribute__((availability(macos,unavailable)));
void f4(gone_t *p) __attribute__((availability(macos,unavailable)));
int f5(void) { return sizeof(gone_t); }
struct gone_s *f6(void);
