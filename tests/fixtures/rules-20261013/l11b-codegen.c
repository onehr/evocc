/* L11b/L11c: code-shape probe; compile with -S only and read the calls and
 * instructions. Not a value test. The last two functions use orders that
 * GCC documents as invalid; the compilers' diagnostics are the observation. */
struct r3 { char c[3]; };
struct r8 { int a, b; };
struct r12 { int a, b, c; };

int fadd(int *p, int v) { return __atomic_fetch_add(p, v, 5); }
unsigned char fnand(unsigned char *p, unsigned char v) { return __atomic_fetch_nand(p, v, 0); }
int xchg(int *p, int v) { return __atomic_exchange_n(p, v, 5); }
int cas(int *p, int *e, int d) { return __atomic_compare_exchange_n(p, e, d, 0, 5, 5); }
int casw(int *p, int *e, int d) { return __atomic_compare_exchange_n(p, e, d, 1, 2, 0); }
int tas(_Bool *p) { return __atomic_test_and_set(p, 5); }
void fence(void) { __atomic_thread_fence(5); }
void fence_acquire(void) { __atomic_thread_fence(2); }
void signal_fence(void) { __atomic_signal_fence(5); }
__int128 load16(__int128 *p) { return __atomic_load_n(p, 5); }
void store16(__int128 *p, __int128 v) { __atomic_store_n(p, v, 5); }
__int128 cas16(__int128 *p, __int128 e, __int128 d)
{ __atomic_compare_exchange_n(p, &e, d, 0, 5, 5); return e; }
__int128 fadd16(__int128 *p, __int128 v) { return __atomic_fetch_add(p, v, 5); }
void load3(struct r3 *p, struct r3 *r) { __atomic_load(p, r, 5); }
void load8(struct r8 *p, struct r8 *r) { __atomic_load(p, r, 5); }
void load12(struct r12 *p, struct r12 *r) { __atomic_load(p, r, 5); }
int cas8(struct r8 *p, struct r8 *e, struct r8 *d)
{ return __atomic_compare_exchange(p, e, d, 0, 5, 5); }
int bad_load(int *p) { return __atomic_load_n(p, 3); }
int bad_failure(int *p, int *e, int d)
{ return __atomic_compare_exchange_n(p, e, d, 0, 0, 2); }
