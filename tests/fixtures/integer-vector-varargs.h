typedef int iv4 __attribute__((vector_size(16)));
typedef short sv4 __attribute__((vector_size(8)));
struct hv2 { sv4 a, b; };
long vector_variadic(iv4 named, int n, ...);
long vector_hva_variadic(int n, ...);
