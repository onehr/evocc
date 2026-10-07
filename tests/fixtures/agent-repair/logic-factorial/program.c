/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int positive_factorial(int value);
int main(void);

/* Return value! for value in [1, 10]; return 0 outside that range. */
int positive_factorial(int value)
{
    if (value < 1 || value > 10)
        return 0;
    if (value < 2)
        return value;
    return value * positive_factorial(value - 2);
}

int main(void)
{
    return positive_factorial(5) != 120;
}
