/* L05 probe: floating NEON intrinsic families through <arm_neon.h>.
 * Each CASE is one family; exit status 0 means the checked lanes match.
 *  0 operator forms: vaddq/vsubq/vmulq/vdivq/vnegq_f32, vdupq_n_f32,
 *    vgetq_lane_f32, vsetq_lane_f32
 *  1 memory: vld1q_f32, vst1q_f32, vld1q_f64, vst1q_f64
 *  2 masks and bits: vceqq_f32, vcltq_f32, vreinterpretq_u32_f32,
 *    vreinterpretq_f32_u32, vbslq_f32
 *  3 conversions: vcvtq_s32_f32, vcvtq_f32_s32, vcvt_f64_f32, vcvt_f32_f64
 *  4 multiply-add: vfmaq_f32 must fuse; vmlaq_f32 result is reported:
 *    exit 0 unfused, 4 fused
 *  5 abs/max/min/sqrt: vabsq_f32, vmaxq_f32, vminq_f32, vmaxnmq_f32,
 *    vsqrtq_f32 (vmaxq propagates NaN, vmaxnmq prefers the number)
 *  6 reductions: vaddvq_f32, vpaddq_f32, vmaxvq_f32
 *  7 estimates (observation): exit 0 when vrecpeq_f32(3) and
 *    vrsqrteq_f32(4) give the FRECPE/FRSQRTE table values
 *  8 rounding: vrndq/vrndnq/vrndmq/vrndpq/vrndaq_f32
 *  9 lane moves: vget_low/high_f32, vcombine_f32, vextq_f32,
 *    vrev64q_f32, vzip1q_f32 */
#include <arm_neon.h>
#ifndef CASE
#define CASE 0
#endif
typedef union { float f; unsigned u; } F;
static volatile float one = 1.0f;

int main(void)
{
    float32x4_t a = {1, -2, 3, -4};
    a = vmulq_f32(a, vdupq_n_f32(one));
#if CASE == 0
    float32x4_t b = vdupq_n_f32(2);
    float32x4_t r = vnegq_f32(vdivq_f32(vmulq_f32(vsubq_f32(vaddq_f32(a, b), b), b), b));
    r = vsetq_lane_f32(9.0f, r, 3);
    return vgetq_lane_f32(r, 0) != -1 || vgetq_lane_f32(r, 2) != -3
        || vgetq_lane_f32(r, 3) != 9;
#elif CASE == 1
    float in[4] = {1, 2, 3, 4}, out[4];
    double din[2] = {5, 6}, dout[2];
    vst1q_f32(out, vaddq_f32(vld1q_f32(in), a));
    vst1q_f64(dout, vld1q_f64(din));
    return out[0] != 2 || out[1] != 0 || out[3] != 0 || dout[1] != 6;
#elif CASE == 2
    F q = {.u = 0x7fc00000u};
    float32x4_t b = {1, q.f, 4, -5};
    uint32x4_t eq = vceqq_f32(a, b), lt = vcltq_f32(a, b);
    uint32x4_t bits = vreinterpretq_u32_f32(a);
    float32x4_t sel = vbslq_f32(lt, a, b);
    float32x4_t back = vreinterpretq_f32_u32(bits);
    return eq[0] != 0xffffffffu || eq[1] != 0 || lt[1] != 0 || lt[2] != 0xffffffffu
        || bits[1] != 0xc0000000u || sel[2] != 3 || sel[3] != -5 || back[3] != -4;
#elif CASE == 3
    float32x4_t h = {1.5f, -2.5f, 3e9f, -0.0f};
    h = vmulq_f32(h, vdupq_n_f32(one));
    int32x4_t i = vcvtq_s32_f32(h);
    float32x4_t f = vcvtq_f32_s32((int32x4_t){16777217, -1, 7, 0});
    float64x2_t d = vcvt_f64_f32(vget_low_f32(a));
    float32x2_t n = vcvt_f32_f64((float64x2_t){0.1, 2.0});
    F n0 = {.f = n[0]};
    return i[0] != 1 || i[1] != -2 || i[2] != 2147483647 || f[0] != 16777216.0f
        || d[1] != -2.0 || n0.u != 0x3dcccccdu;
#elif CASE == 4
    F x = {.u = 0x3f800800u}, z = {.u = 0xbf801000u};
    float32x4_t xv = vdupq_n_f32(x.f), zv = vdupq_n_f32(z.f);
    float32x4_t fused = vfmaq_f32(zv, xv, xv);
    float32x4_t mla = vmlaq_f32(zv, xv, xv);
    F fu = {.f = fused[0]}, ml = {.f = mla[0]};
    if (fu.u != 0x33800000u)
        return 1;
    return ml.u == 0 ? 0 : ml.u == 0x33800000u ? 4 : 2;
#elif CASE == 5
    F q = {.u = 0x7fc00000u};
    float32x4_t b = {q.f, 5, -7, -4};
    float32x4_t mx = vmaxq_f32(a, b), mn = vminq_f32(a, b), nm = vmaxnmq_f32(a, b);
    float32x4_t s = vsqrtq_f32(vabsq_f32((float32x4_t){-4, 9, -16, 0.25f}));
    return mx[0] == mx[0] || mx[1] != 5 || mn[2] != -7 || nm[0] != 1 || s[0] != 2
        || s[1] != 3 || s[3] != 0.5f;
#elif CASE == 6
    float32x4_t p = vpaddq_f32(a, a);
    return vaddvq_f32(a) != -2 || p[0] != -1 || p[1] != -1 || vmaxvq_f32(a) != 3;
#elif CASE == 7
    F r = {.f = vgetq_lane_f32(vrecpeq_f32(vdupq_n_f32(3 * one)), 0)};
    F s = {.f = vgetq_lane_f32(vrsqrteq_f32(vdupq_n_f32(4 * one)), 0)};
    return (r.u != 0x3eaa8000u) | (s.u != 0x3eff8000u) << 1;
#elif CASE == 8
    float32x4_t h = {2.5f, -2.5f, 1.5f, -0.5f};
    h = vmulq_f32(h, vdupq_n_f32(one));
    float32x4_t t = vrndq_f32(h), n = vrndnq_f32(h), m = vrndmq_f32(h);
    float32x4_t p = vrndpq_f32(h), r = vrndaq_f32(h);
    return t[1] != -2 || n[0] != 2 || n[2] != 2 || m[1] != -3 || p[0] != 3
        || r[0] != 3 || r[3] != -1;
#elif CASE == 9
    float32x2_t lo = vget_low_f32(a), hi = vget_high_f32(a);
    float32x4_t c = vcombine_f32(hi, lo);
    float32x4_t e = vextq_f32(a, c, 1);
    float32x4_t v = vrev64q_f32(a);
    float32x4_t z = vzip1q_f32(a, c);
    return c[0] != 3 || c[3] != -2 || e[0] != -2 || e[3] != 3 || v[0] != -2
        || v[3] != 3 || z[1] != 3 || z[2] != -2;
#else
#error unsupported CASE
#endif
}
