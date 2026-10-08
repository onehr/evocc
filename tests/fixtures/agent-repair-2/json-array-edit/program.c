/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
typedef struct Item Item;
struct Item
{
    Item *next;
    Item *prev;
    Item *child;
    unsigned value;
};
static void clear_item(Item *item, unsigned value)
{
    item->next = 0;
    item->prev = 0;
    item->child = 0;
    item->value = value;
}
static Item *get_array_item(Item *array, unsigned index)
{
    Item *child = array ? array->child : 0;
    while (child != 0 && index > 0)
    {
        index--;
        child = child->next;
    }
    return child;
}
static int append_item(Item *array, Item *item)
{
    Item *child = 0;
    if (array == 0 || item == 0 || array == item)
    {
        return 0;
    }
    child = array->child;
    if (child == 0)
    {
        array->child = item;
        item->prev = item;
        item->next = 0;
    }
    else
    {
        Item *tail = child->prev;
        if (tail == 0)
        {
            return 0;
        }
        tail->next = item;
        item->prev = tail;
        item->next = 0;
        child->prev = item;
    }
    return 1;
}
static Item *detach_item(Item *parent, Item *item)
{
    if (parent == 0 ||
        (item != parent->child && item->prev == 0))
    {
        return 0;
    }
    if (item != parent->child)
    {
        item->prev->next = item->next;
    }
    if (item->next != 0)
    {
        item->next->prev = item->prev;
    }
    if (item == parent->child)
    {
        parent->child = item->next;
    }
    else if (item->next == 0)
    {
        parent->child->prev = item;
    }
    item->prev = 0;
    item->next = 0;
    return item;
}
static int insert_item(Item *array, unsigned which, Item *newitem)
{
    Item *after = 0;
    if (array == 0 || newitem == 0)
    {
        return 0;
    }
    after = get_array_item(array, which);
    if (after == 0)
    {
        return append_item(array, newitem);
    }
    if (after != array->child && after->prev == 0)
    {
        return 0;
    }
    newitem->next = after;
    newitem->prev = after->prev;
    after->prev = newitem;
    if (after == array->child)
    {
        array->child = newitem;
    }
    else
    {
        newitem->prev->next = newitem;
    }
    return 1;
}
static int replace_item(Item *parent, Item *item, Item *replacement)
{
    if (parent == 0 || parent->child == 0 || replacement == 0 || item == 0)
    {
        return 0;
    }
    if (replacement == item)
    {
        return 1;
    }
    replacement->next = item->next;
    replacement->prev = item->prev;
    if (replacement->next != 0)
    {
        replacement->next->prev = replacement;
    }
    if (parent->child == item)
    {
        if (parent->child->prev == parent->child)
        {
            replacement->prev = replacement;
        }
        parent->child = replacement;
    }
    else
    {
        replacement->prev->next = replacement;
        if (replacement->next == 0)
        {
            parent->child->prev = replacement;
        }
    }
    item->next = 0;
    item->prev = 0;
    return 1;
}
static unsigned array_hash(Item *array)
{
    Item *current = array->child;
    Item *previous = 0;
    Item *tail = 0;
    unsigned count = 0;
    unsigned hash = 5;
    unsigned valid = 1;
    while (current != 0 && count < 16)
    {
        if (previous != 0 && current->prev != previous)
        {
            valid = 0;
        }
        hash = (hash * 131u + current->value) & 65535u;
        previous = current;
        tail = current;
        current = current->next;
        count++;
    }
    if (current != 0)
    {
        return 11;
    }
    if (array->child != 0 && array->child->prev != tail)
    {
        valid = 0;
    }
    return (hash + count * 17u + valid * 71u) & 65535u;
}
static unsigned remove_at(Item *array, unsigned index)
{
    Item *item = detach_item(array, get_array_item(array, index));
    if (item == 0)
    {
        return 3;
    }
    return item->value * 7u + (unsigned)(item->next == 0 && item->prev == 0);
}
static unsigned array_count(Item *array)
{
    Item *current = array->child;
    unsigned count = 0;
    while (current != 0 && count < 16)
    {
        count++;
        current = current->next;
    }
    return count;
}
unsigned evaluate(unsigned seed)
{
    Item array;
    Item values[10];
    unsigned i = 0;
    unsigned count = 3u + seed % 4u;
    unsigned result = 0;
    clear_item(&array, 0);
    for (i = 0; i < 10; i++)
    {
        clear_item(values + i, seed * 13u + i * 7u + 1u);
    }
    for (i = 0; i < count; i++)
    {
        result += (unsigned)append_item(&array, values + i);
    }
    result += array_hash(&array);
    result += remove_at(&array, count - 1u);
    result += (unsigned)append_item(&array, values + 6) * 11u;
    result += array_hash(&array) * 17u;
    result += (unsigned)insert_item(&array, 0, values + 7) * 19u;
    result += (unsigned)insert_item(&array, 2, values + 8) * 23u;
    result += (unsigned)replace_item(&array, get_array_item(&array, 1), values + 9);
    result += array_hash(&array) * 31u;
    result += remove_at(&array, seed == 11u ? 20u : 2u);
    result += array_count(&array) * 37u;
    return result & 65535u;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
