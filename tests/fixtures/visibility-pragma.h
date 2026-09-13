#pragma GCC visibility push(hidden)
extern int hidden_data;
extern int hidden_function(int);
extern int weak_hidden_data __attribute__((weak));
int hidden_definition = 7;
#pragma GCC visibility push(default)
extern int default_data;
#pragma GCC visibility pop
extern int restored_hidden_data;
#pragma GCC visibility pop
extern int restored_default_data;
