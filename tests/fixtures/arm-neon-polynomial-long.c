#ifdef TEST_PRIVATE
typedef __Poly64_t poly64_t;
typedef __Poly128_t poly128_t;
typedef __Poly64x2_t poly64x2_t;
typedef __Uint64x2_t uint64x2_t;
#define vmull_p64 __builtin_aarch64_crypto_pmulldi_ppp
#define vmull_high_p64 __builtin_aarch64_crypto_pmullv2di_ppp
#else
#include <arm_neon.h>
#endif

static int calls;
static poly64_t once(unsigned long value) { ++calls; return (poly64_t)value; }
static unsigned __int128 reference(unsigned long a, unsigned long b) {
    unsigned __int128 result = 0;
    for (int i=0; i<64; ++i) if (b >> i & 1) result ^= (unsigned __int128)a << i;
    return result;
}
int main(void) {
    const unsigned long values[] = {0,1,3,0x8000000000000000UL,~0UL,0x123456789abcdef0UL};
    for (int i=0; i<6; ++i) for (int j=0; j<6; ++j) {
        unsigned long a=values[i], b=values[j];
        poly128_t low=vmull_p64(once(a), once(b));
        if (calls != 2*(i*6+j+1)) return 1;
        poly128_t high=vmull_high_p64((poly64x2_t){17,a}, (poly64x2_t){29,b});
        uint64x2_t x=(uint64x2_t)low, y=(uint64x2_t)high;
        unsigned __int128 expected=reference(a,b);
        if (x[0] != (unsigned long)expected || x[1] != (unsigned long)(expected>>64)) return 2;
        if (y[0] != x[0] || y[1] != x[1]) return 3;
    }
    return 0;
}
