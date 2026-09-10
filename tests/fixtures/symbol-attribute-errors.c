#if CASE == 1
int value __attribute__((section("")));
#elif CASE == 2
int value __attribute__((section(".data.good\0ignored")));
#elif CASE == 3
extern int value __attribute__((section(".data.first")));
int value __attribute__((section(".data.second")));
#elif CASE == 4
extern int value __attribute__((visibility("hidden")));
extern int value __attribute__((visibility("default")));
int *address(void) { return &value; }
#elif CASE == 5
extern int value __attribute__((visibility("default")));
extern int value __attribute__((visibility("hidden")));
int *address(void) { return &value; }
#elif CASE == 6
extern int value __attribute__((weak, visibility("hidden")));
int *address(void) { return &value; }
#elif CASE == 7
int value __attribute__((section(".bss.custom"))) = 1;
#else
#error choose a probe case
#endif
