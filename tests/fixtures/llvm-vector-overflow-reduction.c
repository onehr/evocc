/* Source-bound lane-zero reduction of LLVM pr23135.c:131.
 * This companion does not replace or alter the original benchmark.
 * Default: defined widened calculation; -DPROBE_OVERFLOW: the original int
 * multiplication. Both use the same dynamic operands at its first overflow.
 */
volatile int left = 75840000;
volatile int right = 156;

int main(void)
{
#ifdef PROBE_OVERFLOW
    return left * right;
#else
    long long result = (long long)left * right;
    return result != 11831040000LL || result <= 2147483647LL;
#endif
}
