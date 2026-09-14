extern void missing(void);
struct callbacks { void (*event)(void); };
static void event(void) { missing(); }
static const struct callbacks operations = { event };
static inline int register_callbacks(const struct callbacks *unused)
{
    return -1;
}
int main(void) { return register_callbacks(&operations) != -1; }
