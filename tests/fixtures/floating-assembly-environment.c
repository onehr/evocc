#pragma STDC FENV_ACCESS ON
double scalar_asm_round(double x) {
    double y;
    __asm__ ("frintx %d0, %d1" : "=w"(y) : "w"(x));
    return y;
}
float scalar_asm_roundf(float x) {
    float y;
    __asm__ ("frintx %s0, %s1" : "=w"(y) : "w"(x));
    return y;
}
double scalar_asm_ceil(double x) {
    double y;
    __asm__ ("frintp %d0, %d1" : "=w"(y) : "w"(x));
    return y;
}
double scalar_asm_set_round(unsigned long control, double a, double b) {
    __asm__ volatile ("msr fpcr, %0" : : "r"(control));
    return a + b;
}
double scalar_asm_flags(double a, double b) {
    double y;
    __asm__ volatile ("fdiv %d0, %d1, %d2" : "=w"(y) : "w"(a), "w"(b));
    return y;
}
double scalar_asm_copy(double x) {
    double y;
    __asm__ volatile ("fmov %d0, %d1" : "=w"(y) : "w"(x));
    return y;
}
float scalar_asm_copyf(float x) {
    float y;
    __asm__ volatile ("fmov %s0, %s1" : "=w"(y) : "w"(x));
    return y;
}
