int fallback_value = 3;

int * __attribute__((__weak__)) choose(void) { return &fallback_value; }
int ** __attribute__((weak)) choose_pair(void) {
    static int *value = &fallback_value;
    return &value;
}
int * const __attribute__((weak)) choose_qualified(void) { return &fallback_value; }
int (* __attribute__((weak)) callback)(void);
int * __attribute__((weak)) pointer_object;

int *implementation(void) { return &fallback_value; }
extern int * __attribute__((__alias__("implementation"))) alternative(void);

int read_choice(void) { return *choose(); }
