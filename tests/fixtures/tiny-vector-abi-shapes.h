/* L06: every 1-, 2- and 4-byte integer vector shape at a call boundary, as
 * a register argument, a stack argument, an anonymous argument, a result, a
 * round trip and a record member (EvoIR 10.1.14, EvoMIR 6). */
typedef unsigned char b1 __attribute__((vector_size(1)));
typedef unsigned char b2 __attribute__((vector_size(2)));
typedef unsigned char b4 __attribute__((vector_size(4)));
typedef short h1 __attribute__((vector_size(2)));
typedef unsigned short h2 __attribute__((vector_size(4)));
typedef int i1 __attribute__((vector_size(4)));
struct tagged { b4 x; int tag; };

b1 make_b1(void);
b2 make_b2(void);
b4 make_b4(void);
h1 make_h1(void);
h2 make_h2(void);
i1 make_i1(void);
b1 echo_b1(b1 x);
b2 echo_b2(b2 x);
b4 echo_b4(b4 x);
h1 echo_h1(h1 x);
h2 echo_h2(h2 x);
i1 echo_i1(i1 x);
unsigned long take_all(b1 a, b2 b, b4 c, h1 d, h2 e, i1 f);
unsigned long take_stacked(long r0, long r1, long r2, long r3, long r4,
                           long r5, long r6, long r7, b1 a, b4 b, h1 c,
                           i1 d, struct tagged t);
unsigned long take_anonymous(int count, ...);
struct tagged echo_tagged(struct tagged t);

/* One hash over the lanes in memory order, shared by both sides. */
static inline unsigned long mix(unsigned long hash, long lane)
{
    return hash * 131u + (unsigned long)lane;
}
