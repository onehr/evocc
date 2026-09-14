struct page { unsigned long flags; int value; };
typedef union {
    struct page **pages;
    void **encoded_pages;
} page_array __attribute__((__transparent_union__));

static int visits;
static struct page **once(struct page **pages)
{
    visits++;
    return pages;
}

int consume(page_array arg, int count)
{
    return arg.pages ? arg.pages[count - 1]->value : 0;
}

union __attribute__((transparent_union)) integer_arg {
    int signed_value;
    unsigned int unsigned_value;
};

int integer_consume(union integer_arg arg) { return arg.signed_value; }

int main(void)
{
    struct page first = { 0x03fffe0000000000UL, 17 };
    struct page second = { 0x03fffe0000000000UL, 29 };
    struct page *pages[] = { &first, &second };
    struct region { struct page **pages; } region = { pages };
    int (*indirect)(page_array, int) = consume;
    page_array wrapped = { .pages = pages };
    if (consume(region.pages, 2) != 29) return 1;
    if (consume(once(pages), 1) != 17 || visits != 1) return 2;
    if (indirect((void **)pages, 2) != 29) return 3;
    if (consume(wrapped, 1) != 17) return 4;
    if (consume(0, 1) != 0 || consume((void *)0, 1) != 0) return 5;
    if (integer_consume(31) != 31 || integer_consume(47U) != 47) return 6;
    return 0;
}
