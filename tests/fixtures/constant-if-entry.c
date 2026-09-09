int constant_arms(void)
{
    int value = 7;
    int side = 0;
    if (__builtin_classify_type(value) == 8)
        value = 5.25;
    else
        ++value;
    if (__builtin_classify_type(value) == 1)
        value += 2;
    else
        value = 5.25;
    if (0 && ++side)
        value = 5.25;
    if (1 || ++side)
        value += 3;
    else
        value = 5.25;
    return value != 13 || side != 0;
}

int enter_then(void)
{
    int value = 0;
    goto enter;
    if (0) {
    enter:
        value += 7;
    }
    return value != 7;
}

int enter_else(void)
{
    int value = 0;
    goto enter;
    if (1)
        value = 99;
    else {
    enter:
        value += 3;
    }
    return value != 3;
}

int main(void)
{
    return constant_arms() || enter_then() || enter_else();
}
