typedef struct { int counter; } atomic_t;
typedef struct refcount_struct { atomic_t refs; } refcount_t;
enum record_state { EMPTY, READY };
struct common_value {
    refcount_t refcnt;
    enum record_state state;
};
struct wrapper {
    struct common_value common;
    const refcount_t *reference;
    refcount_t array[2];
    unsigned long wide;
    unsigned long long wider;
};
struct wrapper object;
int main(void) { return object.common.refcnt.refs.counter; }
