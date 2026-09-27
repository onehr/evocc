/* A bounded inline-admission query must not recurse on the host stack merely
 * because the source has a long left-associated expression. */
#define TEN n+n+n+n+n+n+n+n+n+n
#define HUNDRED TEN+TEN+TEN+TEN+TEN+TEN+TEN+TEN+TEN+TEN
#define SIX_HUNDRED HUNDRED+HUNDRED+HUNDRED+HUNDRED+HUNDRED+HUNDRED
static unsigned walk(unsigned);
static inline unsigned visit(unsigned n)
{
    return n > 1 ? walk(n - 1) : (SIX_HUNDRED);
}
static unsigned walk(unsigned n) { return visit(n); }
int main(void) { return walk(3) != 600; }
