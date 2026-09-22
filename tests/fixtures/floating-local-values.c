static double local_loop(double input, int n)
{
    double result = input;
    for (int i = 0; i < n; i++) {
        float step = (float)i + 0.25f;
        result += step;
    }
    return result;
}
static float local_branch(float input, int choice)
{
    float result;
    if (choice) result = -input;
    else result = input;
    return result;
}
int main(void)
{
#ifdef UNINITIALIZED
    double value;
    return value != 0.0;
#else
    return local_loop(1.0, 4) != 8.0
        || !__builtin_signbit(local_branch(0.0f, 1))
        || __builtin_signbit(local_branch(-0.0f, 1));
#endif
}
