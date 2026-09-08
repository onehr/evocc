enum negative { NEGATIVE = (-2147483647 - 1) };
enum positive { POSITIVE = 1 };
enum other_negative { OTHER_NEGATIVE = -1 };
enum __attribute__((packed)) small { SMALL = 255 };
enum __attribute__((packed)) signed_small { SIGNED_SMALL = -1 };
enum wide { WIDE_LOW = -1, WIDE_HIGH = 0x100000000L };

_Static_assert(__builtin_types_compatible_p(enum negative, int), "signed mode");
_Static_assert(__builtin_types_compatible_p(enum positive, unsigned int), "unsigned mode");
_Static_assert(__builtin_types_compatible_p(enum small, unsigned char), "packed mode");
_Static_assert(__builtin_types_compatible_p(enum signed_small, signed char), "packed signed mode");
_Static_assert(__builtin_types_compatible_p(enum wide, long), "wide mode");
_Static_assert(!__builtin_types_compatible_p(enum negative, unsigned int), "not just width");
_Static_assert(!__builtin_types_compatible_p(enum negative, enum other_negative), "nominal tags");
_Static_assert(__builtin_types_compatible_p(enum negative *, int *), "pointed-to mode");

int main(void) {
    enum negative e = NEGATIVE;
    int i = -7;
    enum negative *q = &e;
    int *p = &i;
    if (*(1 ? q : p) >= 0 || *(0 ? q : p) != -7) return 1;
    if (*(1 ? p : q) != -7 || *(0 ? p : q) >= 0) return 2;
    p = q;
    *p = -11;
    return e != -11;
}
