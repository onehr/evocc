void (*__attribute__((section(".data.callbacks"))) callback)(void);
int *__attribute__((section(".data.pointer"))) before_name;
int *after_name __attribute__((section(".data.after")));
int *__attribute__((section(".init.text"))) pointer_result(void) { return 0; }

int main(void)
{
    return callback != 0 || before_name != 0 || after_name != 0 || pointer_result() != 0;
}
