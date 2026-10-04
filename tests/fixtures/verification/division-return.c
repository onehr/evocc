int signed_division(int a, int b) { return a / b; }
int signed_remainder(int a, int b) { return a % b; }
unsigned unsigned_division(unsigned a, unsigned b) { return a / b; }
unsigned unsigned_remainder(unsigned a, unsigned b) { return a % b; }
long compound_division(long a, long b) { a /= b; return a; }
unsigned long compound_remainder(unsigned long a, unsigned long b) {
    a %= b;
    return a;
}
