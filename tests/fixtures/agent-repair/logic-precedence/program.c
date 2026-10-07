/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int multiply_then_add(int a, int b, int c);
int main(void);

/* Return a plus the product b*c for inputs in [-100, 100]; return 0 if any input is outside that range. */
int multiply_then_add(int a, int b, int c)
{
    if (a < -100 || a > 100 || b < -100 || b > 100 || c < -100 || c > 100)
        return 0;
    return (a + b) * c;
}

int main(void)
{
    return multiply_then_add(12, 34, 56) != 1916;
}
