/* Reduced from KVM's pointer/null guards: !x must not materialize x != 0
 * merely to compare that boolean with zero again. Keep effects and widths. */
int null_case(int *p) {
    if (!p) return 17;
    return *p;
}
int integer_case(int x) {
    if (!x) return 19;
    return 23;
}
int narrow_case(signed char x) {
    if (!x) return 29;
    return 31;
}
int wide_case(unsigned __int128 x) {
    if (!x) return 37;
    return 41;
}
int function_case(int (*f)(int)) {
    if (!f) return 73;
    return f(0);
}
int truth_value(unsigned x) { return !x; }
int floating_case(double x) {
    if (!x) return 43;
    return 47;
}
volatile int reads;
int evaluated_once(void) {
    if (!(reads++)) return 53;
    return 59;
}
int short_circuit(int y) {
    if (y || !(reads++)) return 61;
    return 67;
}
int main(void) {
    int x = 71;
    if (null_case(0) != 17 || null_case(&x) != 71) return 1;
    if (integer_case(0) != 19 || integer_case(-1) != 23) return 2;
    if (narrow_case(0) != 29 || narrow_case(-128) != 31) return 3;
    if (wide_case(0) != 37 || wide_case((unsigned __int128)1 << 100) != 41) return 4;
    if (floating_case(0.0) != 43 || floating_case(-0.0) != 43) return 5;
    if (floating_case(__builtin_nan("")) != 47 || floating_case(1.0) != 47) return 6;
    reads = 0;
    if (evaluated_once() != 53 || reads != 1) return 7;
    if (evaluated_once() != 59 || reads != 2) return 8;
    if (short_circuit(1) != 61 || reads != 2) return 9;
    if (short_circuit(0) != 67 || reads != 3) return 10;
    if (function_case(0) != 73 || function_case(integer_case) != 19) return 11;
    if (truth_value(0) != 1 || truth_value(0x80000000u) != 0) return 12;
    return 0;
}
