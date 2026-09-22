static int local_counter __attribute__((section(".data..percpu")));
int public_counter __attribute__((section(".data..percpu")));
int hidden_counter __attribute__((visibility("hidden"), section(".data..percpu")));
int protected_counter __attribute__((visibility("protected"), section(".data..percpu")));
static int inherited_counter __attribute__((section(".data..percpu")));
extern int inherited_counter;

static int inherited_function(int value);
int inherited_function(int value)
{
    return value + inherited_counter;
}

int hidden_function(int value) __attribute__((visibility("hidden")));
int hidden_function(int value)
{
    return value + hidden_counter;
}

int protected_function(int value) __attribute__((visibility("protected")));
int protected_function(int value)
{
    return value + protected_counter;
}

static int local_function(int value)
{
    return value + local_counter;
}

int (*local_address(void))(int)
{
    return local_function;
}

int inspect_counters(void)
{
    return local_function(public_counter) + hidden_function(0) + protected_function(0)
        + inherited_function(0);
}
