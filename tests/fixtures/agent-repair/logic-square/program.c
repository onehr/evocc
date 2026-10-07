/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int square_value(int value);
int main(void);

/* Return the square of value in [-1000, 1000]; return 0 outside that range. */
int square_value(int value)
{
    if (value < -1000 || value > 1000)
        return 0;
    return value + value;
}

int main(void)
{
    return square_value(7) != 49;
}
