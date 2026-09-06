/* The control for signed-shift-trap.c: one bit less, so the shift stays inside
 * int64_t and the program is defined. EvoCC, GCC-14 and Clang-19 all print
 * `checksum = 40000000ffffffff`, which places the trap on the shift and not on
 * anything else the two programs share.
 */
#include "csmith.h"
static int64_t g_1 = 0;
int main (int argc, char* argv[])
{
    g_1 = (((int64_t)1) << (sizeof(int64_t)*CHAR_BIT-2));
    transparent_crc(g_1, "g_1", 0);
    platform_main_end(crc32_context ^ 0xFFFFFFFFUL, 0);
}
