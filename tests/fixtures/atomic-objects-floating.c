/* L11a: loads and stores of floating _Atomic objects move their bits with
 * seq_cst atomic accesses. Floating compound assignment stays E2702.
 * Exit 0 is the asserted answer. */
_Atomic double d = 1.5;
_Atomic float f;
static volatile _Atomic double v;

int main(void)
{
    d = 2.25;
    f = (float)d;
    v = d + f;
    double copy = v;
    return d != 2.25 || f != 2.25f || copy != 4.5;
}
