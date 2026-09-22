__thread int tls_value __attribute__((aligned(64)))=7;
extern __thread int tls_alias __attribute__((alias("tls_value")));
int main(void) {
    if(&tls_alias!=&tls_value)return 1;
    if((unsigned long)&tls_value%64)return 2;
    tls_alias=9;
    return tls_value-9;
}
