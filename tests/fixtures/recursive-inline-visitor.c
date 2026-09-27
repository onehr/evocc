/* A single-site inline visitor can call the out-of-line recursive walk.
 * The substitution graph is finite even though the runtime call graph cycles.
 * Callback-visible local storage, early returns and effects must stay intact.
 */
struct context { unsigned depth, item, before, after; };
static unsigned seen;
static int observe(struct context *p)
{
    if (p->before != seen || p->after != seen + 1 || p->item >= 3)
        return -1;
    ++seen;
    return 0;
}
static int walk(unsigned depth, int (*callback)(struct context *));
static inline int visit(unsigned depth, unsigned item,
                        int (*callback)(struct context *))
{
    struct context ctx = { depth, item, seen, seen + 1 };
    int result = callback(&ctx);
    if (result)
        return result;
    if (ctx.depth)
        return walk(ctx.depth - 1, callback);
    return 0;
}
static int walk(unsigned depth, int (*callback)(struct context *))
{
    for (unsigned i = 0; i < 3; ++i) {
        int result = visit(depth, i, callback);
        if (result)
            return result;
    }
    return 0;
}
static int stop(struct context *p)
{
    (void)p;
    return -17;
}
int main(void)
{
    if (walk(3, observe) || seen != 120)
        return 1;
    if (walk(2, stop) != -17 || seen != 120)
        return 2;
    return 0;
}
