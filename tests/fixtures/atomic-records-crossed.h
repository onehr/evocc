/* L11c: shared declarations of the crossed generic-atomic record test. Every
 * record is naturally aligned: its alignment is at least its size of 1, 2, 4
 * or 8 bytes. GCC 14, Clang 19, Apple clang and EvoCC then use lock-free
 * instructions for the generic __atomic forms on all of them. */
#define CAT2(a, b) a##_##b
#define CAT(a, b) CAT2(a, b)
#define N 100000

struct __attribute__((aligned(8))) w8 { int count, check; };
struct __attribute__((aligned(4))) w4 { short count, check; };
struct __attribute__((aligned(2))) w2 { unsigned char count, check; };
struct w1 { unsigned char count; };
/* Alignment 4; the _Atomic object below has alignment 8 on both targets. */
struct pair { int count, check; };
/* A lock record: held is 0 or 1, owner names the last holder. */
struct __attribute__((aligned(8))) lock { int held, owner; };

extern struct w8 c8;
extern struct w4 c4;
extern struct w2 c2;
extern struct w1 c1;
extern _Atomic struct pair cp;
extern struct lock guard;
extern long guarded;
