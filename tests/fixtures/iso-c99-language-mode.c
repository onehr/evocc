#if __STDC_VERSION__ != 199901L || __STRICT_ANSI__ != 1
#error wrong ISO C99 macros
#endif
#if !defined(__GNUC_STDC_INLINE__) || defined(__GNUC_GNU_INLINE__)
#error wrong inline default
#endif

/* These names are not GNU keywords in the strict ISO language mode. */
int asm = 3, typeof = 4;
static inline int sum(int *restrict p) {
  __typeof__(*p) n = 0;
  for (int i = 0; i < *p; ++i) n += i;
  __asm__("" : "+r"(n));
  return n;
}
int main(void) { int n = 4; return asm + typeof != 7 || sum(&n) != 6; }
