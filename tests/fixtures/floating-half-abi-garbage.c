/* L03: a half argument with garbage above its 16 bits. AAPCS64 C.1 leaves
 * the bits of a vector register above a half argument unspecified, so a
 * callee must read the low 16 bits alone. This caller passes binary32
 * values whose low half is the half argument and whose high half is
 * garbage, through a function pointer of the binary32 signature: the
 * calling convention of the two signatures places each value in the same
 * register. Build it with a reference compiler and link it with the
 * worker of floating-half-abi; main returns 0 when the callee ignores the
 * garbage. */
#include <string.h>
#include "floating-half-abi.h"

static float garbage(u16 half, unsigned salt)
{
    unsigned raw = (0x5a5au ^ salt) << 16 | half;
    float value;
    memcpy(&value, &raw, sizeof value);
    return value;
}

static u16 low(float value)
{
    unsigned raw;
    memcpy(&raw, &value, sizeof raw);
    return (u16)raw;
}

int main(void)
{
    unsigned failed = 0;
    float (*second)(float, float) = (float (*)(float, float))(void (*)(void))w_second;
    if (low(second(garbage(0x3c00, 1), garbage(0x3c05, 2))) != 0x3c05)
        failed |= 1;
    unsigned (*mixed)(float, float, double, float, int, float) =
        (unsigned (*)(float, float, double, float, int, float))(void (*)(void))w_mixed;
    if (mixed(2.0f, garbage(0x3c01, 3), 3.0, garbage(0x3c02, 4), 7, garbage(0x3c03, 5)) != 0x3f)
        failed |= 2;
    float (*eight)(float, float, float, float, float, float, float, float) =
        (float (*)(float, float, float, float, float, float, float, float))(void (*)(void))w_eight;
    if (low(eight(garbage(0x3c00, 6), garbage(0x3c01, 7), garbage(0x3c02, 8),
                  garbage(0x3c03, 9), garbage(0x3c04, 10), garbage(0x3c05, 11),
                  garbage(0x3c06, 12), garbage(0x3c07, 13))) != 0x3c07)
        failed |= 4;
    return (int)failed;
}
