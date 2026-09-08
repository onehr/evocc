typedef int (*callback)();

static int calls;
static int zero(void) { return 0; }
static int add(int x, int y) { calls++; return x + y; }
static callback choose(void) { return add; }

struct hooks { callback first; callback second; };
static struct hooks shared = {zero, add};
static callback empty;

static int invoke(callback fn) { return fn((signed char)2, (short)3); }
static struct hooks *hooks(void) { return &shared; }
typedef struct hooks *(*get_hooks)();
static get_hooks getter(void) { return hooks; }

int main(void) {
    struct hooks local;
    callback table[2] = {zero, add};
    local.first = choose();
    local.second = table[1];
    if (empty != 0 || table[0]() != 0 || invoke(local.first) != 5) return 1;
    if ((*local.second)((char)4, (short)5) != 9) return 2;
    if (choose()(6, 7) != 13) return 3;
    if (getter()()->first() != 0 || getter()()->second(8, 9) != 17) return 4;
    return calls != 4;
}
