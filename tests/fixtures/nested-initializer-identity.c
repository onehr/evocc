struct registers { int a, b; };
struct bank { struct registers first, second; };
struct chip { struct bank *base; int mode; };
struct device { struct chip *chip; };
struct entry { struct device *device; const char *name; void *address; int direction; };

#define REG_ADDRESS(device, field) \
    ({ struct bank *registers = (device)->chip->base; (void *)&registers->field; })
#define CHANNEL_ADDRESS(device, field, direction) \
    ({ struct registers *channel; \
       if ((device)->chip->mode == 1) channel = REG_ADDRESS(device, first); \
       else if ((direction) == 1) channel = REG_ADDRESS(device, second); \
       else channel = REG_ADDRESS(device, first); \
       (void *)&channel->field; })
#define ENTRY(device, field, direction) \
    { device, #field, CHANNEL_ADDRESS(device, field, direction), direction }

static int total;
__attribute__((noinline)) void consume(struct entry *entries)
{
    total += *(int *)entries[0].address + *(int *)entries[1].address;
}

static void register_entries(struct device *device, int direction)
{
    struct entry entries[] = {
        ENTRY(device, a, direction),
        ENTRY(device, b, direction),
    };
    consume(entries);
}

int main(void)
{
    struct bank bank = { {11, 22}, {33, 44} };
    struct chip chip = { &bank, 0 };
    struct device device = { &chip };
    register_entries(&device, 0);
    register_entries(&device, 1);
    chip.mode = 1;
    register_entries(&device, 1);
    return total != 143;
}
