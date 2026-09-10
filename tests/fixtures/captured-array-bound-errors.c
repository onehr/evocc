#if CASE == 1
int f(int n) { goto label; typedef int A[n]; label: return 0; }
#elif CASE == 2
int f(int n) { goto label; { typedef int A[n]; label: return 0; } }
#elif CASE == 3
int f(int n) { switch(n) { typedef int A[n]; case 1: return 0; } return 0; }
#elif CASE == 4
int f(int *n) { typedef int A[n]; return 0; }
#elif CASE == 5
int f(int n) { typedef int A[n]; _Static_assert(sizeof(A) > 0, "runtime"); return 0; }
#endif
