/* L11a: the definition of the shared object of the crossed _Atomic test.
 * One compiler defines it; the workers of other compilers update it. */
#include "atomic-objects-crossed.h"

int cells[2 * N + 1];
struct holder shared = { .product = 1, .cursor = cells, .wrap = 0 };

void object_layout(long *out)
{
    LAYOUT(out);
}
