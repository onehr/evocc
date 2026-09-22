#ifndef CASE
#define CASE 1
#endif

typedef unsigned short mask_t;
struct request { const mask_t (*masks)[]; };

#if CASE == 1
int main(void) {
    mask_t masks[2] = { 17, 23 };
    struct request r = { .masks = &masks };
    return (*r.masks)[0] != 17 || (*r.masks)[1] != 23;
}
#elif CASE == 2
static void set(struct request *r, mask_t (*masks)[2]) { r->masks = masks; }
int main(void) {
    mask_t masks[2] = { 17, 23 };
    struct request r = { 0 };
    set(&r, &masks);
    return (*r.masks)[1] != 23;
}
#elif CASE == 3
static int read(const volatile mask_t (*masks)[]) { return (*masks)[1]; }
int main(void) {
    mask_t masks[2] = { 17, 23 };
    return read(&masks) != 23;
}
#elif CASE == 4
static const mask_t (*convert(mask_t (*masks)[2]))[] { return masks; }
int main(void) {
    mask_t masks[2] = { 17, 23 };
    return (*convert(&masks))[0] != 17;
}
#elif CASE == 5
int main(void) {
    mask_t masks[2][3] = { { 1, 2, 3 }, { 4, 5, 6 } };
    const mask_t (*p)[][3] = &masks;
    return (*p)[1][2] != 6;
}
#elif CASE == 6
int main(void) {
    const mask_t masks[2] = { 17, 23 };
    mask_t (*p)[] = &masks;
    return (*p)[1] != 23; /* Discarding cv is not permission to write. */
}
#elif CASE == 7
int main(void) {
    mask_t masks[2] = { 17, 23 };
    const mask_t (*p)[3] = &masks; /* Different known extents. */
    return 0;
}
#elif CASE == 8
int main(void) {
    mask_t *masks[2] = { 0 };
    const mask_t *(*p)[] = &masks; /* Qualifiers below a pointer stay significant. */
    return 0;
}
#elif CASE == 9
int main(void) {
    _Atomic mask_t masks[2];
    const mask_t (*p)[] = &masks;
    return 0;
}
#endif

_Static_assert(!__builtin_types_compatible_p(mask_t (*)[2], const mask_t (*)[2]),
               "conversion does not change type-query compatibility");
