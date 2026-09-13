extern void invalid(void);

static int disabled(int ignored) { return 0; }
static int impossible(void) { invalid(); return 1; }

static const unsigned char lengths[] = { 3, 5, 7 };
static __attribute__((always_inline)) inline unsigned total_length(void)
{
    unsigned n = 2;
    for (unsigned i = 0; i < sizeof(lengths); ++i)
        n += lengths[i];
    return n;
}

typedef struct { int counter; } atomic_t;
typedef struct {
    union {
        atomic_t val;
        struct { unsigned char locked, pending; };
        struct { unsigned short locked_pending, tail; };
    };
} spinlock_t;

static inline void check_lock(spinlock_t *lock)
{
    spinlock_t *p = (void *)lock;
    union { unsigned val; spinlock_t lock; } u = { .lock = {{ .val = {0} }} };
    do { if (!(u.val == 0)) invalid(); } while (0);
    switch (sizeof(*p)) { case 4: break; default: invalid(); }
}

int flags(int c, int n)
{
    int value = c ? 1 : 0;
    if (n > 1)
        value |= 2;
    if (value > 3)
        invalid();
    return value;
}

int main(void)
{
    union { unsigned value; struct { unsigned char a, b, c, d; } bytes; }
        lock = { .bytes = { 0, 0, 0, 0 } };
    unsigned long size;
    size = sizeof(long);
    if (!__builtin_constant_p(size) || total_length() > 255)
        invalid();
    if (size > 4096 || lock.value != 0)
        invalid();
    if (disabled(4))
        return impossible();
    spinlock_t spin = {0};
    check_lock(&spin);
    const int code = ({
        int n = -22;
        if (sizeof(long) == 1) n = 16;
        else if (sizeof(long) == 2) n = 8;
        else if (sizeof(long) == 4) n = 0;
        else if (sizeof(long) == 8) n = 24;
        n;
    });
    if (code < 0)
        invalid();
    return total_length() != 17 || code != 24 || flags(0, 1) != 0 || flags(1, 1) != 1
        || flags(0, 2) != 2 || flags(1, 2) != 3;
}
