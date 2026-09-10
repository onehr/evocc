static long object = 7;
static int evaluations;
static volatile long general = 17;
static long *place(void) { ++evaluations; return &object; }

int main(void) {
    long read;
    __asm__ volatile("ldr %0, %1" : "=r"(read) : "m"(*place()));
    __asm__ volatile("str %1, %0" : "=m"(*place()) : "r"(11L));
    __asm__ volatile("ldr x9, %0\n\tadd x9, x9, #3\n\tstr x9, %0"
                     : "+m"(*place()) : : "x9");
    if (read != 7 || object != 14 || evaluations != 3) return 1;

    long local = 23;
    __asm__ volatile("ldr %0, %1" : "=r"(read) : "m"(local));
    if (read != 23) return 2;
    __asm__ volatile("" : : "g"(general++), "g"(&local), "g"(5));
    if (general != 18 || local != 23) return 3;
    return 0;
}
