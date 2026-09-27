/* The compile-time half of -pthread; -D/-U still override predefinitions. */
#ifndef EXPECT_REENTRANT
#define EXPECT_REENTRANT 1
#endif
#if EXPECT_REENTRANT == 0
#ifdef _REENTRANT
#error unexpected_reentrant_macro
#endif
#else
#if !defined(_REENTRANT) || _REENTRANT != EXPECT_REENTRANT
#error missing_or_incorrect_reentrant_macro
#endif
#endif
int main(void) { return 0; }
