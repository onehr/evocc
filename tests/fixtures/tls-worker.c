__thread int tls_shared = 7;
static __thread struct { int n; int a[3]; } state = {11, {2,3,5}};
int *tls_address(void) { return &tls_shared; }
int tls_read(void) { return tls_shared; }
int tls_update(int n) {
    static __thread int calls;
    tls_shared = n;
    state.n += 1;
    calls += 1;
    return state.n + state.a[2] + calls;
}
