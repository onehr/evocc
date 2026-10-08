/* Altered cJSON source; MIT notice retained in ../LICENSE.cjson. */
/* JSON Merge Patch (RFC 7396) over a caller-owned pool of nodes.
 * A patch that is not an object replaces the target. An object patch turns
 * a non-object target into an empty object, then visits its members in
 * order: a null member deletes the key; any other member is merged
 * recursively into the detached old value, and the result is appended as
 * the last member. Keys compare exactly in strict mode and ignore ASCII case
 * otherwise. Child lists keep the cJSON invariant: the first child's prev
 * points to the last child, and the last child's next is null.
 * Only evaluate seeds 0..11 are supported.
 */
enum { cJSON_NULL = 4, cJSON_Number = 8, cJSON_String = 16, cJSON_Array = 32, cJSON_Object = 64 };
enum { POOL_SIZE = 64, KEY_SIZE = 8 };

typedef struct cJSON cJSON;
struct cJSON {
    cJSON *next;
    cJSON *prev;
    cJSON *child;
    int type;
    int valueint;
    char string[KEY_SIZE];
    char valuestring[KEY_SIZE];
};
typedef struct { cJSON nodes[POOL_SIZE]; int used; } pool_t;
typedef struct { char *buffer; int length; int offset; } printbuffer;

/* Null when the pool is exhausted. */
static cJSON *cJSON_New_Item(pool_t *pool)
{
    cJSON *node;
    int i;
    if (pool->used >= POOL_SIZE)
        return 0;
    node = &pool->nodes[pool->used++];
    node->next = node->prev = node->child = 0;
    node->type = cJSON_NULL;
    node->valueint = 0;
    for (i = 0; i < KEY_SIZE; i++)
        node->string[i] = node->valuestring[i] = 0;
    return node;
}

/* Copies at most KEY_SIZE - 1 characters. */
static void copy_text(char *to, const char *from)
{
    int i;
    for (i = 0; i + 1 < KEY_SIZE && from[i] != 0; i++)
        to[i] = from[i];
    to[i] = 0;
}

static int lower(int c)
{
    return (c >= 'A' && c <= 'Z') ? c + 32 : c;
}

static int compare_strings(const char *string1, const char *string2, int case_sensitive)
{
    if (case_sensitive) {
        for (; *string1 == *string2; string1++, string2++)
            if (*string1 == 0)
                return 0;
        return (unsigned char)*string1 - (unsigned char)*string2;
    }
    for (; lower(*string1) == lower(*string2); string1++, string2++)
        if (*string1 == 0)
            return 0;
    return lower(*string1) - lower(*string2);
}

static int cJSON_IsObject(const cJSON *item)
{
    return (item->type & 0xFF) == cJSON_Object;
}

static int cJSON_IsNull(const cJSON *item)
{
    return item != 0 && (item->type & 0xFF) == cJSON_NULL;
}

static cJSON *get_object_item(const cJSON *object, const char *name, int case_sensitive)
{
    cJSON *current_element;
    if (object == 0 || name == 0)
        return 0;
    current_element = object->child;
    while (current_element != 0 && compare_strings(name, current_element->string, case_sensitive) != 0)
        current_element = current_element->next;
    return current_element;
}

static void suffix_object(cJSON *prev, cJSON *item)
{
    prev->next = item;
    item->prev = prev;
}

/* Append item; the head's prev locates the last child. */
static int add_item_to_array(cJSON *array, cJSON *item)
{
    cJSON *child;
    if (item == 0 || array == 0 || array == item)
        return 0;
    child = array->child;
    if (child == 0) {
        array->child = item;
        item->prev = item;
        item->next = 0;
    } else if (child->prev) {
        suffix_object(child->prev, item);
        array->child->prev = item;
    }
    return 1;
}

static int add_item_to_object(cJSON *object, const char *string, cJSON *item)
{
    if (object == 0 || string == 0 || item == 0 || object == item)
        return 0;
    copy_text(item->string, string);
    return add_item_to_array(object, item);
}

/* Unlink item from parent and keep the head/last invariant. */
static cJSON *cJSON_DetachItemViaPointer(cJSON *parent, cJSON *item)
{
    if (parent == 0 || item == 0 || (item != parent->child && item->prev == 0))
        return 0;
    if (item != parent->child)
        item->prev->next = item->next;
    if (item->next != 0)
        item->next->prev = item->prev;
    if (item == parent->child)
        parent->child = item->next;
    else if (item->prev == 0)
        parent->child->prev = item->prev;
    item->prev = 0;
    item->next = 0;
    return item;
}

static cJSON *detach_item_from_object(cJSON *object, const char *string, int case_sensitive)
{
    return cJSON_DetachItemViaPointer(object, get_object_item(object, string, case_sensitive));
}

/* Deep copy; the copy keeps the key. */
static cJSON *cJSON_Duplicate(pool_t *pool, const cJSON *item)
{
    cJSON *newitem;
    const cJSON *child;
    if (item == 0)
        return 0;
    newitem = cJSON_New_Item(pool);
    if (newitem == 0)
        return 0;
    newitem->type = item->type;
    newitem->valueint = item->valueint;
    copy_text(newitem->string, item->string);
    copy_text(newitem->valuestring, item->valuestring);
    for (child = item->child; child != 0; child = child->next) {
        cJSON *newchild = cJSON_Duplicate(pool, child);
        if (newchild == 0)
            return 0;
        add_item_to_array(newitem, newchild);
    }
    return newitem;
}

/* Returns the merged value, or null when the pool is exhausted. Detached
 * and replaced nodes stay in the pool unused. */
static cJSON *merge_patch(pool_t *pool, cJSON *target, const cJSON *patch, int case_sensitive)
{
    const cJSON *patch_child;
    if (!cJSON_IsObject(patch))
        return cJSON_Duplicate(pool, patch);
    if (!cJSON_IsObject(target)) {
        target = cJSON_New_Item(pool);
        if (target == 0)
            return 0;
        target->type = cJSON_Object;
    }
    for (patch_child = patch->child; patch_child != 0; patch_child = patch_child->next) {
        if (cJSON_IsNull(patch_child)) {
            detach_item_from_object(target, patch_child->string, case_sensitive);
        } else {
            cJSON *replace_me = detach_item_from_object(target, patch_child->string, case_sensitive);
            cJSON *replacement = merge_patch(pool, replace_me, patch_child, case_sensitive);
            if (replacement == 0)
                return 0;
            add_item_to_object(target, patch_child->string, replacement);
        }
    }
    return target;
}

/* Compact printing; returns 0 when the buffer is too small. */
static int put(printbuffer *p, const char *text)
{
    while (*text) {
        if (p->offset + 1 >= p->length)
            return 0;
        p->buffer[p->offset++] = *text++;
    }
    p->buffer[p->offset] = 0;
    return 1;
}

static int print_number(int value, printbuffer *p)
{
    char digits[12];
    int pos = 11;
    unsigned magnitude = value < 0 ? 0u - (unsigned)value : (unsigned)value;
    digits[pos] = 0;
    do {
        digits[--pos] = (char)('0' + magnitude % 10);
        magnitude /= 10;
    } while (magnitude != 0);
    if (value < 0)
        digits[--pos] = '-';
    return put(p, digits + pos);
}

static int print_string_ptr(const char *text, printbuffer *p)
{
    return put(p, "\"") && put(p, text) && put(p, "\"");
}

static int print_value(const cJSON *item, printbuffer *p);

static int print_children(const cJSON *item, printbuffer *p, int with_keys)
{
    const cJSON *current = item->child;
    if (!put(p, with_keys ? "{" : "["))
        return 0;
    for (; current != 0; current = current->next) {
        if (with_keys && !(print_string_ptr(current->string, p) && put(p, ":")))
            return 0;
        if (!print_value(current, p) || (current->next != 0 && !put(p, ",")))
            return 0;
    }
    return put(p, with_keys ? "}" : "]");
}

static int print_value(const cJSON *item, printbuffer *p)
{
    switch (item->type & 0xFF) {
    case cJSON_NULL: return put(p, "null");
    case cJSON_Number: return print_number(item->valueint, p);
    case cJSON_String: return print_string_ptr(item->valuestring, p);
    case cJSON_Array: return print_children(item, p, 0);
    case cJSON_Object: return print_children(item, p, 1);
    default: return 0;
    }
}

static cJSON *add(pool_t *pool, cJSON *parent, const char *key, int type, int value)
{
    cJSON *item = cJSON_New_Item(pool);
    if (item == 0)
        return 0;
    item->type = type;
    item->valueint = value;
    if (type == cJSON_String) {
        item->valuestring[0] = (char)('a' + value % 26);
        item->valuestring[1] = (char)('a' + value / 26 % 26);
    }
    if (parent != 0 && !((parent->type & 0xFF) == cJSON_Array ? add_item_to_array(parent, item)
                                                               : add_item_to_object(parent, key, item)))
        return 0;
    return item;
}

/* Target: {"id":n,"tags":[..],"meta":{"v":n,"w":"s"},"name":"s"[,"Zip":n]}.
 * Patch members come from mask bits; a later bit may touch a key that an
 * earlier bit added or removed. */
static cJSON *build(pool_t *pool, unsigned seed, int is_patch)
{
    unsigned mask = (seed * 2654435761u >> 7) & 0x3FF;
    cJSON *root = add(pool, 0, 0, cJSON_Object, 0);
    cJSON *meta, *tags;
    if (!is_patch) {
        add(pool, root, "id", cJSON_Number, (int)seed * 7 - 20);
        tags = add(pool, root, "tags", cJSON_Array, 0);
        add(pool, tags, 0, cJSON_Number, (int)seed);
        add(pool, tags, 0, cJSON_String, (int)seed + 3);
        meta = add(pool, root, "meta", cJSON_Object, 0);
        add(pool, meta, "v", cJSON_Number, (int)seed * 3);
        add(pool, meta, "w", cJSON_String, (int)seed * 5);
        add(pool, root, "name", cJSON_String, (int)seed * 11);
        if (seed % 3 == 0)
            add(pool, root, "Zip", cJSON_Number, 90000 + (int)seed);
        return root;
    }
    if (mask & 1) add(pool, root, "name", cJSON_String, (int)seed + 40);
    if (mask & 2) add(pool, root, "ID", cJSON_Number, (int)seed);
    if (mask & 4) add(pool, add(pool, root, "meta", cJSON_Object, 0), "v", cJSON_NULL, 0);
    if (mask & 8) add(pool, root, "tags", cJSON_Number, -(int)seed);
    if ((mask & 24) == 24) add(pool, add(pool, root, "geo", cJSON_Object, 0), "lat", cJSON_Number, (int)seed * 13);
    if (mask & 32) add(pool, root, "zip", cJSON_NULL, 0);
    if (mask & 64) add(pool, add(pool, root, "meta", cJSON_Object, 0), "x", cJSON_String, (int)seed);
    if (mask & 128) add(pool, root, "name", cJSON_NULL, 0);
    if (mask & 256) add(pool, root, "extra", cJSON_Array, 0);
    if (mask & 512) add(pool, root, "id", cJSON_Number, 1000 - (int)seed);
    return root;
}

static unsigned long long print_hash(const cJSON *item, unsigned long long acc)
{
    char text[200];
    printbuffer p;
    int i;
    p.buffer = text;
    p.length = (int)sizeof(text);
    p.offset = 0;
    if (item == 0 || !print_value(item, &p))
        return (acc ^ 0xBADu) * 0x100000001B3ULL;
    for (i = 0; i < p.offset; i++)
        acc = (acc ^ (unsigned char)text[i]) * 0x100000001B3ULL;
    return (acc ^ (unsigned)p.offset) * 0x100000001B3ULL;
}

/* Merge the patch into the target, then once more into the result; a merge
 * patch is idempotent. Even seeds compare keys strictly. */
unsigned long long evaluate(unsigned seed)
{
    pool_t pool;
    int case_sensitive = seed % 2 == 0;
    cJSON *target, *patch, *result;
    unsigned long long acc = 0xCBF29CE484222325ULL;
    pool.used = 0;
    target = build(&pool, seed, 0);
    patch = build(&pool, seed, 1);
    result = merge_patch(&pool, target, patch, case_sensitive);
    acc = print_hash(result, acc);
    result = merge_patch(&pool, result, patch, case_sensitive);
    acc = print_hash(result, acc);
    return (acc ^ (unsigned)pool.used) * 0x100000001B3ULL;
}

int main(void)
{
    unsigned long long checksum = evaluate(4);
    checksum ^= evaluate(8) * 3;
    return (int)(checksum & 127);
}
