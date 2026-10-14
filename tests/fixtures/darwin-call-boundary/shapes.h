/* Call shapes across the compiler boundary on arm64 macOS (T03h, D04).
   callee.c defines each function; caller.c calls each one with fixed values
   and checks the result. One compiler builds callee.c and the other builds
   caller.c, in both directions. */

typedef signed char sc;
typedef unsigned char uc;
typedef unsigned short us;
typedef int v2i __attribute__((vector_size(8)));
typedef int v4i __attribute__((vector_size(16)));

struct r3 { char c[3]; };
struct r12 { int a, b, c; };
struct a16 { _Alignas(16) long a; long b; };
struct h3 { float a, b, c; };
struct h2d { double a, b; };

/* Narrow arguments: the callee may rely on the caller's extension. */
int takes_sc(sc c);
int takes_char(char c);
int takes_short(short s);
int takes_uc(uc c);
int takes_us(us s);
int takes_bool(_Bool b);
int takes_mixed(sc a, short b, uc c, us d, char e);

/* Narrow results: the caller may rely on the callee's extension. */
sc gives_sc(int x);
char gives_char(int x);
short gives_short(int x);
uc gives_uc(int x);
us gives_us(int x);
_Bool gives_bool(int x);

/* Pointers to callees with narrow parameters and results. */
typedef int (*narrow_fn)(sc);
typedef sc (*narrow_result_fn)(int);
narrow_fn pick_takes_sc(void);
narrow_result_fn pick_gives_sc(void);

/* 128-bit pairs: no even-register rounding. */
long pair_after_one(long a, __int128 b);
long pair_after_three(long a, long b, long c, __int128 d, long e);
__int128 pair_result(long a, __int128 b);

/* Stack arguments: natural size and alignment. */
float tenth(float a, float b, float c, float d, float e, float f, float g, float h,
            float i, float j);
long ints(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7,
          sc s8, uc u9, short s10, us u11, int i12, sc s13, long l14, char c15);
double floats(double d0, double d1, double d2, double d3, double d4, double d5, double d6,
              double d7, float f8, double d9, float f10, float f11, double d12, float f13,
              float f14, double d15);
long records(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7, char c8,
             struct r3 r, char c9, struct r12 q, char c10, struct a16 al, char c11);
double floating_records(double d0, double d1, double d2, double d3, double d4, double d5,
                        double d6, double d7, float f8, struct h3 h, float f9, struct h2d g,
                        float f10);
long pair_on_stack(long a0, long a1, long a2, long a3, long a4, long a5, long a6, long a7,
                   char c8, __int128 w, char c9);
long record_split(long a0, long a1, long a2, long a3, long a4, long a5, long a6,
                  struct r12 q, long z);
long interleaved(int i0, double d0, sc i1, float d1, short i2, double d2, int i3, float d3,
                 long i4, double d4, uc i5, float d5, us i6, double d6, int i7, float d7,
                 sc i8, float d8, short i9, double d9, char i10, float d10);
long vectors(v4i a, v4i b, v4i c, v4i d, v4i e, v4i f, v4i g, v4i h, v2i i, v4i j, v2i k);
