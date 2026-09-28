/* Without diagnostic options this valid GNU C program returns zero.
 * With -Werror=unused-variable, both fixed references reject it.
 * CASE=1 instead exercises -pedantic-errors on a GNU statement expression.
 * The command projector must preserve either rejection request. */
#ifndef CASE
#define CASE 0
#endif
int main(void)
{
#if CASE == 0
    int deliberately_unused = 1;
    return 0;
#else
    return ({ int result = 0; result; });
#endif
}
