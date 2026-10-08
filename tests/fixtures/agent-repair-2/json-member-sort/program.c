/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* Sort caller-owned member lists by ASCII name, case-sensitively or with
 * ASCII case folding. Missing names sort last; equal keys keep insertion order.
 * Preserve each member's value and coherent forward/backward links.
 * Only evaluate seeds 0..11 are supported.
 */
typedef struct Member Member;
struct Member
{
    Member *next;
    Member *prev;
    const unsigned char *name;
    unsigned value;
};
static unsigned lower_byte(unsigned byte)
{
    if (byte >= 'A' && byte <= 'Z')
    {
        return byte + ('a' - 'A');
    }
    return byte;
}
static int compare_names(const unsigned char *a, const unsigned char *b,
                         int sensitive)
{
    if (a == b)
    {
        return 0;
    }
    if (a == 0)
    {
        return 1;
    }
    if (sensitive)
    {
        while (*a == *b)
        {
            if (*a == 0)
            {
                return 0;
            }
            a++;
            b++;
        }
        return (int)*a - (int)*b;
    }
    for (; lower_byte(*a) == lower_byte(*b); a++, b++)
    {
        if (*a == 0)
        {
            return 0;
        }
    }
    return (int)lower_byte(*a) - (int)lower_byte(*b);
}
static Member *sort_list(Member *list, int sensitive)
{
    Member *first = list;
    Member *second = list;
    Member *current = list;
    Member *result = list;
    Member *tail = 0;
    if (list == 0 || list->next == 0)
    {
        return result;
    }
    while (current != 0 && current->next != 0 &&
           compare_names(current->name, current->next->name, sensitive) <= 0)
    {
        current = current->next;
    }
    if (current == 0 || current->next == 0)
    {
        return result;
    }
    current = list;
    while (current != 0)
    {
        second = second->next;
        current = current->next;
        if (current != 0)
        {
            current = current->next;
        }
    }
    if (second != 0 && second->prev != 0)
    {
        second->prev->next = 0;
        second->prev = 0;
    }
    first = sort_list(first, sensitive);
    second = sort_list(second, sensitive);
    result = 0;
    while (first != 0 && second != 0)
    {
        Member *smaller = 0;
        if (compare_names(first->name, second->name, sensitive) < 0)
        {
            smaller = first;
        }
        else
        {
            smaller = second;
        }
        if (result == 0)
        {
            tail = smaller;
            result = smaller;
        }
        else
        {
            tail->next = smaller;
            smaller->prev = tail;
            tail = smaller;
        }
        if (first == smaller)
        {
            first = first->next;
        }
        else
        {
            second = second->next;
        }
    }
    if (first != 0)
    {
        if (result == 0)
        {
            return first;
        }
        tail->next = first;
        first->prev = tail;
    }
    if (second != 0)
    {
        if (result == 0)
        {
            return second;
        }
        tail->next = second;
        second->prev = tail;
    }
    return result;
}
static void initialize(Member *nodes, unsigned count, unsigned seed, int reverse)
{
    const unsigned char *names[7] = {
        (const unsigned char *)"b",
        (const unsigned char *)"A",
        (const unsigned char *)"a",
        (const unsigned char *)"b",
        (const unsigned char *)"C",
        (const unsigned char *)"a",
        (const unsigned char *)"d"
    };
    unsigned i = 0;
    for (i = 0; i < count; i++)
    {
        unsigned selected = (i + seed) % 7u;
        if (reverse)
        {
            selected = (count - i - 1u + seed) % 7u;
        }
        nodes[i].name = names[selected];
        nodes[i].value = seed * 17u + i + 1u;
        nodes[i].next = i + 1u < count ? nodes + i + 1 : 0;
        nodes[i].prev = i > 0 ? nodes + i - 1 : 0;
    }
    if (seed == 11u)
    {
        nodes[count - 1u].name = 0;
    }
}
static unsigned name_hash(const unsigned char *name)
{
    unsigned result = 7;
    if (name == 0)
    {
        return 127;
    }
    while (*name != 0)
    {
        result = (result * 31u + *name) & 65535u;
        name++;
    }
    return result;
}
static unsigned list_hash(Member *head, int sensitive)
{
    Member *current = head;
    Member *previous = 0;
    Member *tail = 0;
    unsigned count = 0;
    unsigned backward = 0;
    unsigned hash = 3;
    while (current != 0 && count < 8)
    {
        if (current->prev != previous)
        {
            return 13;
        }
        if (previous != 0 &&
            compare_names(previous->name, current->name, sensitive) > 0)
        {
            return 19;
        }
        hash = (hash * 131u + name_hash(current->name) + current->value) & 65535u;
        previous = current;
        tail = current;
        current = current->next;
        count++;
    }
    if (current != 0)
    {
        return 23;
    }
    current = tail;
    while (current != 0 && backward < 8)
    {
        hash = (hash * 17u + current->value) & 65535u;
        current = current->prev;
        backward++;
    }
    if (backward != count || current != 0)
    {
        return 29;
    }
    return (hash + count * 37u) & 65535u;
}
unsigned evaluate(unsigned seed)
{
    Member first[8];
    Member second[8];
    Member *a = 0;
    Member *b = 0;
    unsigned count = 3u + seed % 5u;
    int sensitive = (int)(seed % 2u);
    unsigned hash = 0;
    initialize(first, count, seed, 0);
    initialize(second, count, seed, 1);
    a = sort_list(first, sensitive);
    b = sort_list(second, sensitive);
    hash = list_hash(a, sensitive);
    hash = (hash * 31u + list_hash(b, sensitive)) & 65535u;
    a = sort_list(a, sensitive);
    hash = (hash + list_hash(a, sensitive) * 7u) & 65535u;
    return hash;
}
int main(void)
{
    unsigned result = evaluate(0);
    result ^= evaluate(1);
    result ^= evaluate(2);
    result ^= evaluate(11);
    return (int)(result & 255u);
}
