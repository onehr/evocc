/* Branching callees must resume the caller with the selected return value. */
int branch_value(int condition, int x)
{
    return condition ? x + 1 : x - 1;
}

int branch_result(int condition, int x)
{
    return branch_value(condition, x) + 2;
}

int branch_argument(int condition, int x)
{
    return branch_value(branch_value(condition, x), x);
}

int branch_short_left(int condition, int x)
{
    return branch_value(condition, x) && branch_value(!condition, x);
}

int branch_local(int condition, int x)
{
    int y = branch_value(condition, x);
    return y ? (y += branch_value(1, 2), y) : y;
}
