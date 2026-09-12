__attribute__((noinline)) long add(long n) { return n + 1; }

__attribute__((noinline)) long update(long n) {
    long live = add(n);
    register long value asm("x30") = n;
    asm volatile("add %0, %0, #3" : "+r"(value));
    long result = value;
    return result + add(live);
}

__attribute__((noinline)) long tied(long n) {
    register long value asm("x30");
    asm volatile("add %0, %1, #5" : "=r"(value) : "0"(n));
    return value;
}

__attribute__((noinline)) long branch(long n) {
    register long value asm("x30") = n;
    asm goto("add %0, %0, #7\n cbnz %1, %l[yes]"
             : "+r"(value) : "r"(n) : : yes);
    return value;
yes:
    return value + 10;
}

int main(void) {
    return update(11) != 27 || tied(4) != 9 || branch(0) != 7 || branch(1) != 18;
}
