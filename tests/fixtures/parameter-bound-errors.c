#if CASE == 1
void f(int a[1.5]);
#elif CASE == 2
void f(int a[(int *)0]);
#elif CASE == 3
void side(void);
void f(int a[side()]);
#endif
