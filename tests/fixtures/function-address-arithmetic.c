typedef long (*function)(long);
static long add(long x) { return x + 3; }
static function saved = add;
static int evaluations;
static function *cell(void) { ++evaluations; return &saved; }
static long amount(void) { ++evaluations; return 4; }

int main(void) {
    function p = add;
    _Static_assert(__builtin_types_compatible_p(typeof(p - p), long), "difference type");
    if ((p + 9) - p != 9 || p - (p + 9) != -9) return 1;
    function q = 4 + p;
    q -= 4;
    if (q(4) != 7) return 2;
    q += (short)-2;
    q = q - (signed char)-2;
    if (q != p) return 3;
    if (q++ != p || q - p != 1) return 4;
    if (--q != p || ++q != p + 1) return 5;
    if (q-- != p + 1 || q != p) return 6;
    struct holder { function value; } h = {p};
    h.value += 6;
    h.value--;
    h.value -= 5;
    if (h.value(8) != 11) return 7;
    function array[1] = {p};
    ++array[0];
    --array[0];
    if (array[0] != p) return 8;
    (*cell()) += amount();
    if (evaluations != 2 || saved - p != 4) return 9;
    saved -= 4;
    if (saved(10) != 13) return 10;
    if ((unsigned long)(p + 3) - (unsigned long)p != 3) return 11;
    return 0;
}
