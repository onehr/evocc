const static int values[] = { 3, 7 };
unsigned static long const count = 2;
typedef struct item { int value; } item_t;
struct item const static tag_value = { 5 };
item_t const static alias_value = { 9 };
int static inline read_value(void) { return values[1]; }

int main(void) {
    const static int local[] = { 11, 13 };
    return values[0] != 3 || count != 2 || tag_value.value != 5 ||
           alias_value.value != 9 || read_value() != 7 || local[1] != 13;
}
