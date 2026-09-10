extern int hidden_value;
extern int hidden_value __attribute__((visibility("hidden")));
extern int hidden_function(void) __attribute__((visibility("hidden")));
extern int default_value;

__attribute__((section(".data..percpu"), aligned(64))) int counter;
__attribute__((section(".data.rel.ro.refs"))) int *reference = &counter;
__attribute__((visibility("hidden"))) int local_definition = 7;
__attribute__((visibility("hidden"))) int hidden_definition(void) { return 29; }
extern int alternate(void) __attribute__((alias("hidden_definition"), visibility("hidden")));

int *hidden_address(void) { return &hidden_value; }
int *default_address(void) { return &default_value; }
int call_hidden(void) { return hidden_function(); }
int read_counter(void) { return counter + local_definition; }
