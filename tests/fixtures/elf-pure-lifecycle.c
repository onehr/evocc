static void before(void) __attribute__((constructor(201)));
static void before(void) {}

static __attribute__((constructor(202))) void on_definition(void) {
    (void)0;
}

static void after(void) {}
static void after(void) __attribute__((destructor(201)));

int main(void) { return 0; }
