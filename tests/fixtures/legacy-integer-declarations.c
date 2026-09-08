static object;
static counter;
extern object;

first(a, b) short a; unsigned char b;
{
    return a + b;
}

twice(x)
{
    return x * 2;
}

static inline local()
{
    return 1;
}

int main(void)
{
    object = 9;
    counter = object + local();
    if (first(258, 259) != 261 || twice(4) != 8 || counter != 10)
        return 1;
    return later(3) != 7;
}

later(value) int value;
{
    return value * 2 + 1;
}
