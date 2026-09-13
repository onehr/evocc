/* Reproduces the scope-exit operation used by Linux scoped_guard. */
static int held;
static int releases;
static int order;

struct guard { int *lock; };

static struct guard acquire(void)
{
    held = 1;
    return (struct guard){ &held };
}

static void release(struct guard *g)
{
    *g->lock = 0;
    releases++;
}

static void record(int *value)
{
    order = order * 10 + *value;
}

static int returning(void)
{
    int value __attribute__((cleanup(record))) = 4;
    return value + 1;
}

int main(void)
{
    {
        struct guard g __attribute__((cleanup(release))) = acquire();
        if (held != 1)
            return 1;
    }
    if (held != 0 || releases != 1)
        return 2;
    {
        int a __attribute__((cleanup(record))) = 1;
        int b __attribute__((cleanup(record))) = 2;
    }
    if (order != 21)
        return 3;
    if (returning() != 5 || order != 214)
        return 4;
    for (int i = 0; i < 3; ++i) {
        int value __attribute__((cleanup(record))) = i + 1;
        if (i == 0)
            continue;
        if (i == 1)
            break;
    }
    if (order != 21412)
        return 5;
    {
        int value __attribute__((cleanup(record))) = 3;
        goto out;
    }
out:
    return order != 214123;
}
