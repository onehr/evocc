/* The intermediate reduction of seed 92 of the --no-float campaign, from 1,158
 * lines to 25 by deleting lines while the disagreement held. GCC-14 and
 * Clang-19 print `checksum = ffffffff` at -O0 and -O2; EvoCC's binary traps and
 * prints nothing. The remaining call to safe_sub_func_int64_t_s_s is what
 * carries the undefined shift; signed-shift-trap.c isolates it.
 */
#include "csmith.h"
static int32_t g_2 = (-5L);
static int32_t g_38[7][1] = {{0xCEE05243L},{3L},{3L},{0xCEE05243L},{3L},{3L},{0xCEE05243L}};
static int32_t g_82[4][4] = {{0x79375E77L,0x79375E77L,0x79375E77L,0x79375E77L},{0x79375E77L,0x79375E77L,0x79375E77L,0x79375E77L},{0x79375E77L,0x79375E77L,0x79375E77L,0x79375E77L},{0x79375E77L,0x79375E77L,0x79375E77L,0x79375E77L}};
static int32_t g_90 = 1L;
static int32_t *g_120 = &g_82[3][0];
static int32_t *g_161 = &g_38[3][0];
static uint32_t g_391[4][4] = {{0x4A75D80CL,0x4A75D80CL,0x4A75D80CL,0x4A75D80CL},{0x4A75D80CL,0x4A75D80CL,0x4A75D80CL,0x4A75D80CL},{0x4A75D80CL,0x4A75D80CL,0x4A75D80CL,0x4A75D80CL},{0x4A75D80CL,0x4A75D80CL,0x4A75D80CL,0x4A75D80CL}};
static int32_t g_609[9][2] = {{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL},{0x241DA09EL,0x241DA09EL}};
static uint32_t  func_1(void)
{ /* block id: 0 */
    int64_t t_3 = 0;
    int64_t t_4 = 0;
    int32_t t_614 = 0;
    int32_t t_615 = 0;
    uint32_t *l_616 = &g_391[3][2];
    for (g_2 = 0; (g_2 > (-20)); g_2 = safe_sub_func_int64_t_s_s(t_3, g_2, t_4, 5))
    (*g_120) ^= (0x5930A24FL || ((*l_616) &= (safe_add_func_int32_t_s_s(t_614, (g_609[5][0] | g_90), t_615, (*g_161)))));
}
int main (int argc, char* argv[])
{
    int print_hash_value = 0;
    func_1();
    platform_main_end(crc32_context ^ 0xFFFFFFFFUL, print_hash_value);
}
