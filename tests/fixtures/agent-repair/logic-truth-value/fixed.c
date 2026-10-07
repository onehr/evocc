/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int truth_value(int value);
int main(void);

/* Return 1 exactly when value is nonzero, and return 0 otherwise. */
int truth_value(int value)
{
    if (value)
        return 1;
    return 0;
}

int main(void)
{
    return truth_value(-4) != 1;
}
