/* Driver side of the half-format crossed ABI probe (L03). main returns 0
 * when every call agrees, or 1 + the number of the first shape that fails,
 * because an exit status keeps eight bits:
 *   0 named argument and result, 1 shared SIMD cursor, 2 eight arguments,
 *   3 HFAs of one to four members, 4 five-member record, 5 mixed records,
 *   6 promoted anonymous arguments, 7 stack arguments (Linux),
 *   8 unpromoted anonymous halves (Linux, not __fp16), 9 records of both
 *   half formats (Linux). Shape n sets bit n of the mask. */
#include "floating-half-abi.h"

int main(void)
{
    unsigned failed = 0;
    if (bits(w_second(make(0x3c00), make(0x3c05))) != 0x3c05)
        failed |= 1;
    if (w_mixed(2.0f, make(0x3c01), 3.0, make(0x3c02), 7, make(0x3c03)) != 0x3f)
        failed |= 2;
    if (bits(w_eight(make(0x3c00), make(0x3c01), make(0x3c02), make(0x3c03), make(0x3c04),
                     make(0x3c05), make(0x3c06), make(0x3c07))) != 0x3c07)
        failed |= 4;
#if KIND != 2
    {
        struct H1 h1 = w_h1((struct H1){make(0x3c00)});
        struct H2 h2 = w_h2((struct H2){make(0x3c01), make(0x3c02)});
        struct H3 h3 = w_h3((struct H3){make(0x3c01), make(0x3c02), make(0x3c03)});
        struct H4 h4 = w_h4((struct H4){{make(1), make(2), make(3), make(4)}}, make(9));
        if (bits(h1.a) != 0x3c01 || bits(h2.a) != 0x3c02 || bits(h2.b) != 0x3c01
            || bits(h3.a) != 0x3c03 || bits(h3.b) != 0x3c01 || bits(h3.c) != 0x3c02
            || bits(h4.a[0]) != 4 || bits(h4.a[1]) != 3 || bits(h4.a[2]) != 9
            || bits(h4.a[3]) != 1)
            failed |= 8;
    }
#endif
    struct H5 h5 = w_h5((struct H5){make(1), make(2), make(3), make(4), make(5)});
    if (bits(h5.a) != 5 || bits(h5.b) != 4 || bits(h5.c) != 3 || bits(h5.d) != 2
        || bits(h5.e) != 1)
        failed |= 16;
    struct HF hf = w_hf((struct HF){make(0x3c00), 2.0f});
    struct HI hi = w_hi((struct HI){make(0x3c01), 41, make(0x3c02)});
    if (bits(hf.a) != 0x3c01 || hf.f != 3.0f || bits(hi.a) != 0x3c02 || hi.i != 42
        || bits(hi.b) != 0x3c01)
        failed |= 32;
    /* 1.0, 2.0 and 3.0 in the half format. */
    H one = make(KIND == 2 ? 0x3f80 : 0x3c00);
    H two = make(0x4000);
    H three = make(KIND == 2 ? 0x4040 : 0x4200);
#if KIND == 1
    double promoted = w_va_promoted(3, one, two, three);
#else
    double promoted = w_va_promoted(3, (float)one, (float)two, (float)three);
#endif
    if (promoted != 1.0 * 256 + 2.0 * 16 + 3.0)
        failed |= 64;
#ifdef LINUX
    {
        struct HM hm = w_hm((struct HM){f16_make(0x3c00), bf16_make(0x3f80)});
        struct HN hn = w_hn((struct HN){bf16_make(0x4000), f16_make(0x4200), bf16_make(0x4040),
                                        f16_make(0x4400)},
                            (struct HM){f16_make(0x4500), bf16_make(0x40a0)});
        if (f16_bits(hm.a) != 0x3c01 || bf16_bits(hm.b) != 0x3f82 || bf16_bits(hn.a) != 0x4040
            || f16_bits(hn.b) != 0x4400 || bf16_bits(hn.c) != 0x40a0 || f16_bits(hn.d) != 0x4500)
            failed |= 512;
    }
    if (w_stack(make(0x3c00), make(0x3c01), make(0x3c02), make(0x3c03), make(0x3c04),
                make(0x3c05), make(0x3c06), make(0x3c07), make(0x3c08), make(0x3c09)) != 0x3ff)
        failed |= 128;
#if KIND != 1
    if (w_va_half(10, make(0x3c00), make(0x3c01), make(0x3c02), make(0x3c03), make(0x3c04),
                  make(0x3c05), make(0x3c06), make(0x3c07), make(0x3c08), make(0x3c09)) != 0x3ff)
        failed |= 256;
#endif
#endif
    for (int shape = 0; shape < 10; ++shape)
        if (failed >> shape & 1)
            return 1 + shape;
    return 0;
}
