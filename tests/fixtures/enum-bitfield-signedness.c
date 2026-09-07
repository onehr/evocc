enum Position { STATIC, RELATIVE, ABSOLUTE, FIXED };
enum Signed { NEGATIVE = -1, POSITIVE = 1 };
struct Flags { enum Position position : 2; enum Signed signed_value : 2; } flags;
_Static_assert(sizeof(enum Position) == 4, "plain enum layout");
_Static_assert(_Generic(FIXED, int: 1, default: 0), "enumerator remains int");
_Static_assert((enum Position)-1 > 0, "nonnegative enum underlying unsigned");
_Static_assert((enum Signed)-1 < 0, "negative enum underlying signed");
int main(void) {
    flags.position = FIXED;
    flags.signed_value = NEGATIVE;
    if (flags.position != 3 || flags.signed_value != -1) return 1;
    if (flags.position - 4 != -1) return 2;
    enum Position value = flags.position;
    switch (value) { case FIXED: return 0; default: return 3; }
}
