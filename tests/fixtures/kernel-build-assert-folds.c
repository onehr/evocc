/* Reductions of retained BUILD_BUG_ON calls in Landlock and ICMPv6. */
extern void invalid_rule(void);
extern void invalid_ruleset(void);
extern void invalid_destination(void);
extern void invalid_sender(void);

struct rule {
    unsigned long node[3];
    unsigned int layers;
    unsigned long entries[];
};

struct ruleset {
    union {
        unsigned long deferred[4];
        struct {
            unsigned int count;
            unsigned int layers;
        };
    };
    unsigned long entries[];
};

static void check_records(void)
{
    const struct rule rule = { .layers = ~0U };
    const struct ruleset ruleset = { .count = ~0U, .layers = ~0U };
    if (rule.layers < 16)
        invalid_rule();
    if (ruleset.count < 0xffffffffU || ruleset.layers < 16)
        invalid_ruleset();
}

static __attribute__((always_inline)) inline void require_destination(void *p)
{
    if (!p || p == 0 || (void *)0 == p)
        invalid_destination();
}

static void sender(void) { }

static __attribute__((noinline)) int check_sender(void (*fn)(void))
{
    if (fn != sender)
        invalid_sender();
    return 0;
}

int main(void)
{
    struct rule destination;
    check_records();
    require_destination(&destination);
    return check_sender(sender);
}
