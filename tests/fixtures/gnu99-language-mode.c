/* The original Lua Makefile selects GNU99, not the default GNU11 mode. */
#if __STDC_VERSION__ != 199901L || defined(__STRICT_ANSI__)
#error wrong language-version macros
#endif
#if !defined(__GNUC_STDC_INLINE__) || defined(__GNUC_GNU_INLINE__)
#error GNU99 uses ISO inline semantics unless explicitly overridden
#endif

inline int local_inline_body(int n) { return n + 100; }
int sum(int n) {
  int result = 0;
  for (int i = 0; i < n; ++i) result += i;
  return result;
}

/* Both fixed GNU99 implementations admit these later-standard extensions. */
_Static_assert(_Generic(1, int: 1, default: 0), "integer generic");
int main(void) {
  typeof(1) value = sum(4);
  _Bool yes = value == 6;
  return !yes;
}
