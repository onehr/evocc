/* Quiet comparison builtins: ordered/unordered results and one evaluation. */
static int evaluated;
static double value(double x) { ++evaluated; return x; }

int main(void)
{
    double nan = __builtin_nan("");
    if (!__builtin_isless(-1.0f, 0.0)
        || !__builtin_islessequal(-0.0, 0.0f)
        || !__builtin_isgreater(2, 1.0f)
        || !__builtin_isgreaterequal(1.0, 1)
        || !__builtin_islessgreater(-1.0, 1.0)
        || __builtin_islessgreater(-0.0, 0.0))
        return 1;
    if (__builtin_isless(nan, 0.0) || __builtin_islessequal(0.0, nan)
        || __builtin_isgreater(nan, 0.0)
        || __builtin_isgreaterequal(0.0, nan)
        || __builtin_islessgreater(nan, nan)
        || !__builtin_isunordered(nan, 1.0)
        || !__builtin_isunordered(1.0, nan)
        || __builtin_isunordered(1.0, -1.0))
        return 2;
    if (!__builtin_isunordered(value(nan), value(1.0)) || evaluated != 2)
        return 3;
    return 0;
}
