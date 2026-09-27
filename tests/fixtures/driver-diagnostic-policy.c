/* Compile-only probes for the original musl diagnostic options. */
#ifndef CASE
#define CASE 0
#endif

#if CASE == 0
int probe(void) { return undeclared_function(); }
#elif CASE == 1
probe(void) { return 0; }
#elif CASE == 2
void *probe(void *p) { return p + 1; }
#elif CASE == 3
int *probe(unsigned int *p) { return p; }
#elif CASE == 4
int *probe(const int *p) { return p; }
#elif CASE == 5
int *probe(long *p) { return p; }
#elif CASE == 6
int *probe(void) { return 7; }
#elif CASE == 7
typedef int row[2];
row *probe(const row *p) { return p; }
#elif CASE == 8
int probe(void) { return; }
#elif CASE == 9
int (*probe(int (*p)(void)))(void) { return p + 1; }
#else
#error unknown CASE
#endif
