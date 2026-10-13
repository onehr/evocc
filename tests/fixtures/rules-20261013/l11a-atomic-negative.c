/* L11a: compile-only controls, NEG=0..6. Record accept, warn or reject. */
#ifndef NEG
#define NEG 0
#endif
struct pair { int a, b; };
_Atomic struct pair whole;
_Atomic int counter;

#if NEG == 0 /* member of an atomic record: undefined behavior (6.5.2.3p5) */
int f(void) { return whole.a; }
#elif NEG == 1 /* atomic type specifier naming an array type */
_Atomic(int[3]) bad;
#elif NEG == 2 /* atomic type specifier naming a qualified type */
_Atomic(const int) bad;
#elif NEG == 3 /* pointer conversion that drops _Atomic */
int *f(void) { return &counter; }
#elif NEG == 4 /* atomic bit-field */
struct bits { _Atomic int field : 3; };
#elif NEG == 5 /* atomic element of an ordinary array, updated */
_Atomic int cells[2];
int f(void) { return cells[1] += 1; }
#elif NEG == 6 /* _Atomic applied to a function type */
typedef int fn(void);
_Atomic fn *bad;
#else
#error unsupported NEG
#endif
