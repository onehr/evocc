enum Kind { first };
struct Record { unsigned field : 3; };
union Overlay { int i; long l; };
static int side;
static volatile int observed;
static int effect(void) { return ++side; }
extern double floating_effect(void);

_Static_assert(__builtin_classify_type((char)3) == 1, "character");
_Static_assert(__builtin_classify_type((enum Kind)0) == 1, "enum");
_Static_assert(__builtin_classify_type((__int128)1) == 1, "wide integer");
_Static_assert(__builtin_classify_type((const int *)0) == 5, "pointer");
_Static_assert(__builtin_classify_type("string") == 5, "string");
_Static_assert(__builtin_classify_type(effect) == 5, "function");
_Static_assert(__builtin_classify_type(*effect) == 5, "dereferenced function");
_Static_assert(__builtin_classify_type(*(float *)0) == 8, "float");
_Static_assert(__builtin_classify_type(*(double *)0) == 8, "double");
_Static_assert(__builtin_classify_type(*(long double *)0) == 8, "long double");
_Static_assert(__builtin_classify_type(*(_Complex double *)0) == 9, "complex");
_Static_assert(__builtin_classify_type(*(struct Record *)0) == 12, "struct");
_Static_assert(__builtin_classify_type(*(union Overlay *)0) == 13, "union");
_Static_assert(__builtin_classify_type(*(__builtin_va_list *)0) == 12, "va_list");
_Static_assert(__builtin_classify_type(((struct Record *)0)->field) == 1, "bit-field");
_Static_assert(__builtin_classify_type(floating_effect()) == 8, "floating call");
_Static_assert(__builtin_classify_type(side++) == 1, "increment");
_Static_assert(__builtin_classify_type(observed++) == 1, "volatile increment");
_Static_assert(__builtin_classify_type(effect()) == 1, "call");
_Static_assert(sizeof(__builtin_classify_type(side)) == sizeof(int), "int size");
_Static_assert(_Generic(__builtin_classify_type(side), int: 1, default: 0), "int type");
static int folded = __builtin_classify_type((void *)0);

int main(void) {
  int array[2][3];
  int bound[__builtin_classify_type(side++)];
  const int integer = 3;
  _Static_assert(__builtin_classify_type(integer) == 1, "qualified integer");
  _Static_assert(__builtin_classify_type(array) == 5, "array");
  _Static_assert(__builtin_classify_type(array[0]) == 5, "nested array");
  _Static_assert(__builtin_classify_type((struct Record){0}) == 12, "compound literal");
  _Static_assert(__builtin_classify_type(({ ++side; 1; })) == 1, "statement expression");
  _Static_assert(__builtin_classify_type(1.0f) == 8, "float literal");
  _Static_assert(__builtin_classify_type(1.0) == 8, "double literal");
  _Static_assert(__builtin_classify_type(1.0L) == 8, "long double literal");
  return side != 0 || observed != 0 || folded != 5 || sizeof(bound) != sizeof(int);
}
