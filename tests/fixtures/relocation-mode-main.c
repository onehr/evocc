long data = 7;
void callback(void) {}
extern long *object_address(void);
extern void (*function_address(void))(void);
extern long *weak_object_address(void);
extern void (*weak_function_address(void))(void);
int main(void)
{
    return object_address() != &data || function_address() != callback ||
           weak_object_address() != 0 || weak_function_address() != 0;
}
