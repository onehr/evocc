extern void impossible_state(void);
int ticks;
static void tick(void) { ++ticks; }
struct local_state { int state; unsigned long data; };
static __attribute__((always_inline)) inline void advance(struct local_state *s, int retry)
{
    switch (s->state) {
    case 0: impossible_state(); return;
    case 1:
    case 2: s->state = 0; return;
    case 3:
        if (!retry) { s->state = 0; return; }
        break;
    }
    s->state = 2;
}
int run(int retry)
{
    struct local_state s = { .state = 3, .data = 7 };
    for (; s.state != 0; advance(&s, retry))
        tick();
    return ticks;
}
int main(void) { return run(1) != 2; }
