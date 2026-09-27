/* musl's -fexcess-precision=standard requests the existing source-format
 * evaluation contract, not dynamic rounding, fast math or contraction. */
#if __FLT_EVAL_METHOD__ != 0
#error source_format_evaluation_required
#endif

int main(void)
{
    volatile float f = 1.0f, fh = 0x1p-24f;
    volatile double d = 1.0, dh = 0x1p-53;
    volatile long double q = 1.0L, qh = 0x1p-113L;
    if ((f + fh) - f != 0.0f) return 1;
    if ((d + dh) - d != 0.0) return 2;
    if ((q + qh) - q != 0.0L) return 3;

    /* Assignment and an explicit cast both narrow to their semantic type. */
    volatile double above = 1.0 + 0x1.8p-24;
    float assigned = above;
    if (assigned != 0x1.000002p0f || (float)above != assigned) return 4;

    /* The literal must round directly to binary32, not through binary64. */
    float direct = 1.0000000596046447753906250000000000000001f;
    if (direct != 0x1.000002p0f) return 5;
    return 0;
}
