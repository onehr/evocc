/* Reduced from a random program (Csmith 0cdc710315cf, --no-float profile,
 * seed 92) that EvoCC compiled and whose binary then died on SIGTRAP while
 * GCC-14 and Clang-19 both printed `checksum = 80000000ffffffff` at -O0 and
 * -O2. Building the same source with -DCSMITH_MINIMAL -DUSE_MATH_MACROS_NOTMP
 * -DINT_MAX=2147483647 reproduces it.
 *
 * The shift is `((int64_t)1) << 63`: a signed left shift into the sign bit,
 * which ISO C 6.5.7p4 leaves undefined. Both reference compilers fold it to
 * INT64_MIN; EvoCC emits its undefined-behaviour check and traps, so the
 * program prints nothing and exits 133. The construct is not in the generated
 * program but in Csmith's own safe-math macro header, where every overflow
 * guard builds its mask this way, so it reaches EvoCC through the runtime the
 * whole campaign includes. Agreeing oracles therefore do not establish that a
 * program is defined; see the control below, which shifts one bit less and on
 * which all three agree.
 */
#include "csmith.h"
static int64_t g_1 = 0;
int main (int argc, char* argv[])
{
    g_1 = (((int64_t)1) << (sizeof(int64_t)*CHAR_BIT-1));
    transparent_crc(g_1, "g_1", 0);
    platform_main_end(crc32_context ^ 0xFFFFFFFFUL, 0);
}
