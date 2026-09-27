/* Call conversions need the typed machine even when no operand reads memory. */
#ifndef CASE
#define CASE 0
#endif
#if CASE == 0
__attribute__((noinline)) int function(int *unused) { return 0; }
int main(void) { return function(0); }
#elif CASE == 1
__attribute__((noinline)) double function(void) { return 0; }
int main(void) { return function() != 0; }
#elif CASE == 2
__attribute__((noinline)) int function(double unused) { return 0; }
int main(void) { return function(7); }
#elif CASE == 3
__attribute__((noinline)) int *function(void) { return 0; }
int main(void) { return function() != 0; }
#elif CASE == 4
__attribute__((noinline)) int function(__int128 unused) { return 0; }
int main(void) { return function(7); }
#elif CASE == 5
__attribute__((noinline)) int function(int unused[2]) { return 0; }
int main(void) { return function(0); }
#elif CASE == 6
__attribute__((noinline)) int function(int (*unused)(void)) { return 0; }
int main(void) { return function(0); }
#elif CASE == 7
__attribute__((noinline)) int function(double unused) { return 0; }
int main(void) { double uninitialized; return function(uninitialized); }
#else
#error unsupported CASE
#endif
