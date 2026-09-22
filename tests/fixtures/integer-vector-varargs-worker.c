#include "integer-vector-varargs.h"
long vector_variadic(iv4 named, int n, ...) {
    __builtin_va_list ap, copy;
    __builtin_va_start(ap,n);
    __builtin_va_copy(copy,ap);
    iv4 first=__builtin_va_arg(copy,iv4);
    __builtin_va_end(copy);
    long result=named[0]+first[3];
    for(int i=0;i<n;i++) {
        iv4 v=__builtin_va_arg(ap,iv4);
        long x=__builtin_va_arg(ap,long);
        result+=v[i%4]+x;
    }
    __builtin_va_end(ap);
    return result;
}
long vector_hva_variadic(int n, ...) {
    __builtin_va_list ap;
    __builtin_va_start(ap,n);
    long result=0;
    for(int i=0;i<n;i++) {
        struct hv2 h=__builtin_va_arg(ap,struct hv2);
        result+=h.a[0]+h.b[3];
    }
    __builtin_va_end(ap);
    return result;
}
