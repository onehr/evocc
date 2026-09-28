/* Reduced from doomgeneric p_mobj.c: ANG45 * (mthing->angle / 45).
 * Signed multiplication precedes the assignment to unsigned angle_t.
 * This application needs an explicit -fwrapv profile, not disabled guards.
 */
__attribute__((noinline)) unsigned int angle(short degrees)
{
    return 0x20000000 * (degrees / 45);
}
int main(void)
{
    volatile short degrees = 270;
    return angle(degrees) != 0xc0000000U;
}
