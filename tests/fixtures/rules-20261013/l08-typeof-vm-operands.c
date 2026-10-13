/* L08 probe: is the operand of __typeof__ evaluated when its type is
 * variably modified? Each CASE returns 10 * (side effects observed) + a
 * size check digit (1 = the declared object has the expected size).
 * CASE=0 pointer to VLA through a comma, 1 VLA lvalue through a comma,
 * 2 pointer to VLA from a statement expression, 3 decayed array (int *),
 * 4 increment of a pointer-to-VLA variable, 5 conditional with VM arms,
 * 6 cast to a pointer-to-VLA type, 7 sizeof of the typeof type name. */
int main(void)
{
    int n = 3, calls = 0;
    int a[n];
    int (*p)[n] = &a;
    (void)p;
#if CASE == 0
    __typeof__((calls++, &a)) q = &a;
    return 10 * calls + (sizeof(*q) == 3 * sizeof(int));
#elif CASE == 1
    __typeof__(*(calls++, &a)) b;
    return 10 * calls + (sizeof b == 3 * sizeof(int));
#elif CASE == 2
    __typeof__(({ calls++; &a; })) q = &a;
    return 10 * calls + (sizeof(*q) == 3 * sizeof(int));
#elif CASE == 3
    __typeof__(({ calls++; a; })) r = a;
    return 10 * calls + (sizeof r == sizeof(int *));
#elif CASE == 4
    int (*start)[n] = p;
    __typeof__(p++) q = &a;
    return 10 * (int)(p - start) + (sizeof(*q) == 3 * sizeof(int));
#elif CASE == 5
    __typeof__(calls++ ? &a : &a) q = &a;
    return 10 * calls + (sizeof(*q) == 3 * sizeof(int));
#elif CASE == 6
    void *raw = &a;
    __typeof__((int (*)[n])(calls++, raw)) q = &a;
    return 10 * calls + (sizeof(*q) == 3 * sizeof(int));
#elif CASE == 7
    unsigned long size = sizeof(__typeof__((calls++, &a)));
    return 10 * calls + (size == sizeof(int (*)[3]));
#endif
}
