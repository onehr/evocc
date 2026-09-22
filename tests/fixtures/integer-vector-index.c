typedef int v __attribute__((vector_size(16)));
v f(void) { return (v){1, 2, 3, 4}; }
int main(void) { return f()[((__uint128_t)1 << 100)]; }
