extern __thread int tls_shared;
int tls_reference(void) { return tls_shared; }
int *tls_reference_address(void) { return &tls_shared; }
