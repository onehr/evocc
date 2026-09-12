int main(void)
{
    unsigned x = 5;
    __auto_type y = 2;
    (void)x;
    return __builtin_constant_p((long long)y >= 0);
}
