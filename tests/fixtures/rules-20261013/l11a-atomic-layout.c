/* L11a/L11c: _Atomic layout and lock-free answers. Observation probe.
 * -DPRINT prints one line per type. Without PRINT, -DCASE=n returns
 * size * 32 + alignment of type n, so an interpreter can report it. */
#include <stddef.h>
#ifndef CASE
#define CASE 0
#endif

struct c3 { char c[3]; };
struct c2 { char c[2]; };
struct s6 { short s[3]; };
struct i8 { int a, b; };
struct c5 { char c[5]; };
struct i12 { int a, b, c; };
struct l16 { long a, b; };
struct c17 { char c[17]; };

#define TYPES(X) \
    X(0, _Atomic char) X(1, _Atomic short) X(2, _Atomic int) \
    X(3, _Atomic long) X(4, int *_Atomic) X(5, _Atomic float) \
    X(6, _Atomic double) X(7, _Atomic long double) \
    X(8, _Atomic __int128) X(9, _Atomic struct c3) X(10, _Atomic struct s6) \
    X(11, _Atomic struct i8) X(12, _Atomic struct c5) \
    X(13, _Atomic struct i12) X(14, _Atomic struct l16) \
    X(15, _Atomic struct c17) X(16, _Atomic _Bool) X(17, _Atomic struct c2)

#ifdef PRINT
#include <stdio.h>
#define SHOW(n, T) { static T object; \
    printf("%2d %-24s size=%2zu align=%2zu always=%d always_obj=%d is=%d\n", n, #T, \
        sizeof(T), _Alignof(T), (int)__atomic_always_lock_free(sizeof(T), 0), \
        (int)__atomic_always_lock_free(sizeof(T), &object), \
        (int)__atomic_is_lock_free(sizeof(T), &object)); }
int main(void) { TYPES(SHOW) return 0; }
#else
#define PICK(n, T) case n: return (int)(sizeof(T) * 32 + _Alignof(T));
int main(void) { switch (CASE) { TYPES(PICK) } return 255; }
#endif
