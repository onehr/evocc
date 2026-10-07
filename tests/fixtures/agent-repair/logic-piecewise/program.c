/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int piecewise_value(int value);
int main(void);

/* For value in [0, 100], return its square below 5 and three times value otherwise; return 0 outside that range. */
int piecewise_value(int value)
{
    if (value < 0 || value > 100)
        return 0;
    return (value <= 5) ? (value * value) : (value * 3);
}

int main(void)
{
    return piecewise_value(5) != 15;
}
