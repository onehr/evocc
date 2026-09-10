struct empty {};
struct __attribute__((aligned(16))) aligned_empty {};

long empty_arguments(long a, struct aligned_empty f, long b, struct empty e,
                     long c, long d, long g, long h, long i, long j, long k);
struct empty empty_result(struct empty value);

#ifndef ONLY_CALLER
long empty_arguments(long a, struct aligned_empty f, long b, struct empty e,
                     long c, long d, long g, long h, long i, long j, long k) {
    if (sizeof(e) || sizeof(f)) return -1;
    return a + 2*b + 3*c + 4*d + 5*g + 6*h + 7*i + 8*j + 9*k;
}
struct empty empty_result(struct empty value) { return value; }
#endif

#ifndef ONLY_CALLEE
static char no_bytes[0];
static char empty_string[0] = "";
static struct empty empty_global;
static struct empty empty_copy = (struct empty){};
int main(void) {
    char local[0];
    char local_string[0] = "";
    struct empty first = {}, second = first;
    struct aligned_empty aligned = {};
    second = empty_result(first);
    if (sizeof(local) || sizeof(no_bytes) || sizeof(second) || sizeof(empty_global)
        || sizeof(empty_string) || sizeof(empty_copy) || sizeof(local_string)) return 1;
    if (!local || !no_bytes || !&empty_global) return 2;
    if (empty_arguments(1, aligned, 2, first, 3, 4, 5, 6, 7, 8, 9) != 285) return 3;
    return 0;
}
#endif
