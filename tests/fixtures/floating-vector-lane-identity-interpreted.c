/* Run rules-20261013/l05-lane-identity.c in the EvoCC interpreters, whose
 * step budget is 100,000 steps per run. The probe is included unchanged;
 * this main repeats the probe's own check over the rows FIRST to LAST - 1
 * and the columns COLUMN_FIRST to COLUMN_LAST - 1 of its operand pairs, so
 * a few runs together cover every pair. The exit status counts lanes whose
 * bits differ from the scalar operation, as the probe's main does. */
#define main lane_identity_main
#include "rules-20261013/l05-lane-identity.c"
#undef main

#ifndef FIRST
#define FIRST 0
#endif
#ifndef LAST
#define LAST COUNT
#endif
#ifndef COLUMN_FIRST
#define COLUMN_FIRST 0
#endif
#ifndef COLUMN_LAST
#define COLUMN_LAST COUNT
#endif

/* The unions live outside the loops: an interpreter keeps every object
 * that it creates, so a loop allocates nothing it does not need. */
static VB a, b, r;
static MB m;
static SB s;

int main(void)
{
    int bad = 0;
    if (sizeof(MV) != sizeof(V))
        return 254;
    for (unsigned i = FIRST; i < LAST && i < COUNT; ++i)
        for (unsigned j = COLUMN_FIRST; j < COLUMN_LAST && j < COUNT; ++j) {
            for (unsigned k = 0; k < LANES; ++k) {
                a.u[k] = table[(i + k) % COUNT];
                b.u[k] = table[(j + 3 * k) % COUNT];
            }
            for (int w = 0; w < 5; ++w) {
                r.v = arith(w, a.v, b.v);
                for (unsigned k = 0; k < LANES; ++k) {
                    s.t = scalar(w, a.t[k], b.t[k]);
                    bad += s.u != r.u[k];
                }
            }
            for (int w = 0; w < 6; ++w) {
                m.v = compare(w, a.v, b.v);
                for (unsigned k = 0; k < LANES; ++k)
                    bad += scalar_compare(w, a.t[k], b.t[k]) != m.m[k];
            }
        }
    return bad > 253 ? 253 : bad;
}
