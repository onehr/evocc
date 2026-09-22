#if __STDC_VERSION__ != EXPECT_VERSION
#error incorrect version
#endif
#if EXPECT_ISO
#if __STRICT_ANSI__ != 1
#error missing ISO macro
#endif
int asm = 3, typeof = 4;
#else
#ifdef __STRICT_ANSI__
#error unexpected ISO macro
#endif
typeof(1) value = 7;
#endif
_Static_assert(_Generic(1, int: 1, default: 0), "integer");
int main(void) {
#if EXPECT_ISO
  return asm + typeof != 7;
#else
  return value != 7;
#endif
}
