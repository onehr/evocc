/* Adapted from TinyCC tests under GNU LGPL v2.1; see ../LICENSE.tinycc.
 * Modified for the EvoCC agent-repair corpus on 2026-10-07.
 */

int case_value(int value);
int main(void);

/* Return value for switch cases 1 and 2, and 0 for every other input. */
int case_value(int value)
{
    switch (value) {
    case 1:
        return 1;
    case 2:
        return 1;
    default:
        return 0;
    }
}

int main(void)
{
    return case_value(2) != 2;
}
