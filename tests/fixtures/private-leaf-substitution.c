/* K03 page-table predicate reduction. These are ordinary private functions,
 * not always_inline. Substitution must keep the selected read and argument
 * conversion/evaluation; an unselected null-pointer read must stay unselected. */
struct walker { unsigned flags; };
static int continue_walk(const struct walker *w, int result) {
    if (result == -11) return !(w->flags & 1);
    return !result;
}
static unsigned long leaf_mask(unsigned long pte, signed char level) {
    if (level == 3) return 0;
    return (pte & 3) == 3;
}
static int read_once(volatile int *p, int selected) {
    return selected ? (*p)++ : 41;
}
static int checked_add(int x, int y) { return x + y; }
int walk(const struct walker *w, int result) { return continue_walk(w, result); }
unsigned long table(unsigned long pte, int level) { return leaf_mask(pte, level); }
int selected_read(volatile int *p, int selected) { return read_once(p, selected); }
int add(int x, int y) { return checked_add(x, y); }
int main(void) {
    struct walker w = {0};
    if (walk(&w, -11) != 1 || walk(0, 0) != 1 || walk(0, 2)) return 1;
    w.flags = 1;
    if (walk(&w, -11)) return 2;
    if (!table(3, 2) || table(3, 3) || table(2, 2) || table(3, 259)) return 3;
    volatile int value = 7;
    if (selected_read(&value, 1) != 7 || value != 8) return 4;
    if (selected_read(0, 0) != 41 || value != 8) return 5;
    int argument = 9;
    if (add(argument++, 5) != 14 || argument != 10) return 6;
    if (add(-2147483647, -1) != (-2147483647 - 1)) return 7;
    return 0;
}
