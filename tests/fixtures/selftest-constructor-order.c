/* Reduce the frozen kselftest harness's late direction detector.
 * SOURCE_ORDER=1 checks its original expectation; EARLY_DETECTOR is only a
 * causal control, not a patch to the upstream test or its expected output. */
#ifndef SOURCE_ORDER
#define SOURCE_ORDER 0
#endif
#ifdef EARLY_DETECTOR
#define DETECTOR __attribute__((constructor(10000)))
#else
#define DETECTOR __attribute__((constructor))
#endif

struct node { int id; struct node *next; };
static struct node nodes[3];
static struct node *head, *tail;
static int forward, trace;

static void DETECTOR detect_order(void)
{
    forward = 1;
    trace = trace * 10 + 9;
}

static void register_test(int id)
{
    struct node *item = &nodes[id - 1];
    item->id = id;
    trace = trace * 10 + id;
    if (!head) {
        head = tail = item;
    } else if (forward) {
        tail->next = item;
        tail = item;
    } else {
        item->next = head;
        head = item;
    }
}

static void __attribute__((constructor(20000))) pass(void) { register_test(1); }
static void __attribute__((constructor(20000))) fail(void) { register_test(2); }
static void __attribute__((constructor(20000))) timeout(void) { register_test(3); }

int main(void)
{
    int order = 0, count = 0;
    for (struct node *p = head; p; p = p->next) {
        if (++count > 3) return 2;
        order = order * 10 + p->id;
    }
#ifdef EARLY_DETECTOR
    if (!forward || count != 3 || trace != 9123) return 3;
#else
    if (!forward || count != 3 || trace != 1239) return 3;
#endif
    return order == (SOURCE_ORDER ? 123 : 321) ? 0 : 1;
}
