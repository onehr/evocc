/* One subject object for exact crossed numeric-runtime streams. */
#pragma STDC FENV_ACCESS ON
typedef unsigned __int128 U;
long double quad_from_signed(__int128 x) { return x; }
long double quad_from_unsigned(U x) { return x; }
__int128 quad_to_signed(long double x) { return x; }
U quad_to_unsigned(long double x) { return x; }
long double arithmetic(unsigned op,long double a,long double b) {
    switch(op) {case 0:return a+b; case 1:return a-b; case 2:return a*b; default:return a/b;}
}
extern long double fmal(long double,long double,long double);
long double fused(long double a,long double b,long double c) { return fmal(a,b,c); }
int quad_predicate(unsigned op,long double a,long double b) {
    switch(op) {
    case 0:return a==b; case 1:return a!=b; case 2:return a<b;
    case 3:return a<=b; case 4:return a>b; case 5:return a>=b;
    case 6:return __builtin_isless(a,b); case 7:return __builtin_islessequal(a,b);
    case 8:return __builtin_isgreater(a,b); default:return __builtin_isgreaterequal(a,b);
    }
}
