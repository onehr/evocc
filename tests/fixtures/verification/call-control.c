/* Original calling-expression shapes for the core-v1 composition gate. */
int call_step(int x)
{
    return x + 1;
}

int call_choice(int condition, int x)
{
    return condition ? call_step(x) : -call_step(x);
}

int call_and(int condition, int x)
{
    return condition && call_step(x);
}

int call_or(int condition, int x)
{
    return condition || call_step(x);
}

int call_assignment(int x)
{
    int y = 3;
    y += call_step(x);
    return y;
}

int call_comma(int x)
{
    return (call_step(x), x);
}
