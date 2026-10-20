/* L11a: shared declarations of the crossed _Atomic object test. The member
 * offsets of struct holder depend on the atomic layout rule of the target:
 * GCC raises the alignment of a 1-, 2-, 4-, 8- or 16-byte atomic type to its
 * size; Apple clang rounds a size up to 16 bytes to a power of two and uses
 * that size as the alignment. Clang 19 on Linux uses the Apple rule, so its
 * objects of struct holder disagree with GCC's (llvm-project issue #26836);
 * the Linux test crosses with GCC 14 only. */
#define CAT2(a, b) a##_##b
#define CAT(a, b) CAT2(a, b)
#define N 100000
#define LAYOUT_WORDS 24

struct i8 { int a, b; };
struct c3 { char c[3]; };
struct c2 { char c[2]; };
struct s6 { short s[3]; };
struct c5 { char c[5]; };
struct i12 { int a, b, c; };

struct holder {
    char tag;
    _Atomic struct i8 pair;
    char gap1;
    _Atomic struct c3 three;
    char gap2;
    _Atomic struct s6 six;
    char gap3;
    _Atomic struct c5 five;
    char gap4;
    _Atomic struct i12 twelve;
    char gap5;
    _Atomic struct c2 two;
    _Atomic unsigned counter;
    _Atomic unsigned product;
    _Atomic unsigned char small;
    _Atomic signed char wrap;
    _Atomic _Bool flag;
    int *_Atomic cursor;
    _Atomic long wide;
};

extern struct holder shared;
extern int cells[2 * N + 1];

/* The layout of struct holder as the compiler of a unit sees it. */
#define LAYOUT(out) do { \
    int i = 0; \
    (out)[i++] = sizeof(struct holder); \
    (out)[i++] = _Alignof(struct holder); \
    (out)[i++] = __builtin_offsetof(struct holder, pair); \
    (out)[i++] = __builtin_offsetof(struct holder, three); \
    (out)[i++] = __builtin_offsetof(struct holder, six); \
    (out)[i++] = __builtin_offsetof(struct holder, five); \
    (out)[i++] = __builtin_offsetof(struct holder, twelve); \
    (out)[i++] = __builtin_offsetof(struct holder, two); \
    (out)[i++] = __builtin_offsetof(struct holder, counter); \
    (out)[i++] = __builtin_offsetof(struct holder, small); \
    (out)[i++] = __builtin_offsetof(struct holder, flag); \
    (out)[i++] = __builtin_offsetof(struct holder, cursor); \
    (out)[i++] = __builtin_offsetof(struct holder, wide); \
    (out)[i++] = sizeof(_Atomic struct c3) * 32 + _Alignof(_Atomic struct c3); \
    (out)[i++] = sizeof(_Atomic struct s6) * 32 + _Alignof(_Atomic struct s6); \
    (out)[i++] = sizeof(_Atomic struct c5) * 32 + _Alignof(_Atomic struct c5); \
    (out)[i++] = sizeof(_Atomic struct i12) * 32 + _Alignof(_Atomic struct i12); \
    (out)[i++] = sizeof(_Atomic struct i8) * 32 + _Alignof(_Atomic struct i8); \
    (out)[i++] = sizeof(_Atomic struct c2) * 32 + _Alignof(_Atomic struct c2); \
    (out)[i++] = sizeof(_Atomic long double) * 32 + _Alignof(_Atomic long double); \
    (out)[i++] = sizeof(_Atomic(int *)) * 32 + _Alignof(_Atomic(int *)); \
    (out)[i++] = sizeof(_Atomic _Bool) * 32 + _Alignof(_Atomic _Bool); \
    (out)[i++] = sizeof(_Atomic(struct c3)[2]); \
    (out)[i++] = sizeof(_Atomic(struct s6)[2]); \
} while (0)
