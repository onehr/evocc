int pointer_store(int initial, int value)
{
    int x;
    int *p;

    x = initial;
    p = &x;
    *p = value;
    return *p;
}

int main(void)
{
    return pointer_store(4, 0);
}
