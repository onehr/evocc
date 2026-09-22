#if __STDC__ != 1 || __STRICT_ANSI__ != 1 || defined(__STDC_VERSION__)
#error wrong ISO C89 macros
#endif
#if !defined(__GNUC_GNU_INLINE__) || defined(__GNUC_STDC_INLINE__)
#error wrong legacy inline mode
#endif
int inline = 2, restrict = 3, asm = 4, typeof = 5;
/* Old-style declarations and implicit int are part of the selected mode. */
sum(a, b) short a; char b; { return a + b; }
static __inline__ int helper(int x) { return x + 1; }
int main(void) {
    int u8 = 0;
    return sum(inline, restrict) != 5 || asm + typeof != 9 || helper(u8) != 1;
}
