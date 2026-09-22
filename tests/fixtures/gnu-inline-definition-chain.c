static int inline_effect;
static int helper(int x) { return x + ++inline_effect; }
extern inline __attribute__((gnu_inline, always_inline))
int f(int x) { int temporary = helper(x); return temporary + 100; }
int before(int x) { return f(x); }
int f(int x) { int temporary = x + 7; return temporary; }
int after(int x) { return f(x); }
int main(void) {
    int (*address)(int) = f;
    return before(2) != 9 || after(2) != 9 || address(2) != 9 || inline_effect;
}
