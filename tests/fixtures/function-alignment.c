typedef int Function(void);
static int first(void) __attribute__((aligned(256)));
static int first(void) { return 7; }
static int first(void) __attribute__((aligned(64)));
static int second(void) __attribute__((aligned(64)));
static int second(void) { return 9; }
static int another_name(void) __attribute__((alias("first")));

int main(void) {
    Function *volatile alias_value = another_name;
    Function *volatile first_value = first;
    if (__alignof__(first) != 256 || __alignof__(second) != 64) return 1;
    if (__alignof__(Function) != 4 || __alignof__(__typeof__(first)) != 4) return 2;
    if (((unsigned long)first & 255) || ((unsigned long)second & 63)) return 3;
    if (first == second || alias_value != first_value) return 4;
    Function *round_trip = (Function *)(unsigned long)first;
    if (round_trip != first || round_trip() != 7 || another_name() != 7) return 5;
    return 0;
}
