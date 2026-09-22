/* Adjacent ordinary literals preserve exactly the same UTF-8 section name. */
int accented(void) __attribute__((section(".text.é"), noinline));
#ifdef NUMERIC_SECTION_ESCAPE
int accented(void) __attribute__((section(".text.\xc3" "\xa9")));
#else
int accented(void) __attribute__((section(".text." "é")));
#endif
int accented(void) { return 42; }
int main(void) { return accented() != 42; }
