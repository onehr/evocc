/* Literal null comparisons must not require a second register containing zero. */
typedef int (*callback)(int);
int object_equal(void *p);
int object_unequal(void *p);
int function_equal(callback p);
int function_unequal(callback p);

#ifndef DRIVER_ONLY
int object_equal(void *p) { return p == (void *)0; }
int object_unequal(void *p) { return (void *)0 != p; }
int function_equal(callback p) { return p == (callback)0; }
int function_unequal(callback p) { return (callback)0 != p; }
#endif

#ifndef FUNCTIONS_ONLY
static int calls;
static int object;
static int helper(int x) { return x + 1; }
static void *next_object(void) { ++calls; return &object; }
static callback next_function(void) { ++calls; return helper; }
int main(void)
{
    void *volatile objects[] = {0, &object};
    callback volatile functions[] = {0, helper};
    for (int i = 0; i < 2; ++i) {
        if (object_equal(objects[i]) != !i) return 1;
        if (object_unequal(objects[i]) != i) return 2;
        if (function_equal(functions[i]) != !i) return 3;
        if (function_unequal(functions[i]) != i) return 4;
    }
    if (object_equal(next_object()) || calls != 1) return 5;
    if (!function_unequal(next_function()) || calls != 2) return 6;
    void *slot = &object;
    __atomic_store_n(&slot, (void *)0, __ATOMIC_RELEASE);
    if (__atomic_load_n(&slot, __ATOMIC_ACQUIRE) != 0) return 7;
    if (__sync_val_compare_and_swap(&slot, (void *)0, &object) != 0) return 8;
    if (__sync_lock_test_and_set(&slot, (void *)0) != &object) return 9;
    if (slot != 0) return 10;
    return 0;
}
#endif
