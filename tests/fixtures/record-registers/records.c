/* Records of at most sixteen bytes pass in general registers (EvoMIR 6.4,
 * D29). Their padding and the members that the caller never stores travel
 * with them, a pointer member keeps its provenance, and a result returns
 * the same way. main returns 0 when every value is the expected one; GCC 14
 * and Apple clang 21 return 0 too. */
struct p8 { char c; int i; };
struct p16 { char c; short s; int i; long l; };
struct b3 { char a, b, c; };
struct b5 { char a[5]; };
struct h6 { short a; char b; short c; };
struct b7 { char a[7]; };
struct w12 { int a; char b; int c; };
struct pp { int *p; int x; };
struct fi { float f; int i; };
union u4 { char c; int i; };

static int target = 40;

static int take_p8(struct p8 r) { return r.i + r.c; }
static long take_p16(struct p16 r) { return r.l - r.c; }
static int take_b3(struct b3 r) { return r.a * 10 + r.c; }
static int take_b5(struct b5 r) { return r.a[0] + r.a[4]; }
static int take_h6(struct h6 r) { return r.a - r.c; }
static int take_b7(struct b7 r) { return r.a[1] * r.a[6]; }
static int take_w12(struct w12 r) { return r.a + r.c; }
static int take_pp(struct pp r) { return *r.p + 2; }
static int take_fi(struct fi r) { return (int)(r.f * 2.0f); }
static int take_u4(union u4 r) { return r.c; }
static int forward_p8(struct p8 r) { return take_p8(r) * 2; }
static int after_scalars(int a, int b, int c, int d, int e, int f, struct w12 r) { return a + b + c + d + e + f + r.c; }

static struct p8 make_p8(int i) { struct p8 r; r.i = i; return r; }
static struct b3 make_b3(char a) { struct b3 r; r.a = a; r.c = (char)(a + 1); return r; }
static struct pp make_pp(int *p) { struct pp r; r.p = p; return r; }
static struct w12 make_w12(int c) { struct w12 r; r.c = c; return r; }
static struct h6 make_h6(short a) { struct h6 r; r.a = a; return r; }

int main(void)
{
    int local = 7;
    struct p8 a; a.c = 1; a.i = 2;
    struct p16 b; b.c = 3; b.l = 100;
    struct b3 c; c.a = 4; c.c = 5;
    struct b5 d; d.a[0] = 6; d.a[4] = 7;
    struct h6 e; e.a = 300; e.c = 8;
    struct b7 f; f.a[1] = 9; f.a[6] = 10;
    struct w12 g; g.a = 11; g.c = 12;
    struct pp h; h.p = &target;
    struct pp k; k.p = &local;
    struct fi m; m.f = 1.5f;
    union u4 n; n.c = 13;
    int failures = 0;
    failures += take_p8(a) != 3;
    failures += take_p16(b) != 97;
    failures += take_b3(c) != 45;
    failures += take_b5(d) != 13;
    failures += take_h6(e) != 292;
    failures += take_b7(f) != 90;
    failures += take_w12(g) != 23;
    failures += take_pp(h) != 42;
    failures += take_pp(k) != 9;
    failures += take_fi(m) != 3;
    failures += take_u4(n) != 13;
    failures += forward_p8(a) != 6;
    failures += after_scalars(1, 2, 3, 4, 5, 6, g) != 33;
    failures += make_p8(14).i != 14;
    struct b3 q = make_b3(20);
    failures += q.a + q.c != 41;
    failures += *make_pp(&local).p != 7;
    failures += make_w12(15).c != 15;
    failures += make_h6(16).a != 16;
    return failures;
}
