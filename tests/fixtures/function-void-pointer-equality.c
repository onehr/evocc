typedef int (*function_pointer)(void);
static int first(void) { return 7; }
static int second(void) { return 9; }
static int evaluations;
static void *address(void) { evaluations++; return first; }

int main(void) {
    const volatile void *data = first;
    function_pointer callback = first;
    if (address() != callback || evaluations != 1) return 1;
    if (callback != data || data != callback) return 2;
    if (!(data != second) || !(second != data)) return 3;
    callback = 0;
    data = 0;
    if (callback != data || data != callback) return 4;
    if (first == data || data == first) return 5;
    return 0;
}
