extern long data;
extern void callback(void);
extern long weak_data __attribute__((weak));
extern void weak_callback(void) __attribute__((weak));
long *object_address(void) { return &data; }
void (*function_address(void))(void) { return callback; }
long *weak_object_address(void) { return &weak_data; }
void (*weak_function_address(void))(void) { return weak_callback; }
