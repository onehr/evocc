int values[2] = {7, 9};
int result(void) { return 13; }

struct __attribute__((packed)) row {
    unsigned char head;
    int *value;
    int (*function)(void);
    unsigned long address;
    unsigned char tail;
};
static struct row rows[2] = {
    {1, &values[1], result, (unsigned long)&values[0], 2},
    {3, &values[0], result, (unsigned long)&values[1], 4},
};

int main(void)
{
    if (sizeof(rows) != 52 || __builtin_offsetof(struct row, value) != 1
        || __builtin_offsetof(struct row, function) != 9
        || __builtin_offsetof(struct row, address) != 17
        || rows[0].head != 1 || rows[0].tail != 2
        || rows[1].head != 3 || rows[1].tail != 4
        || rows[0].function() != 13 || rows[1].function() != 13
        || *(int *)rows[0].address != 7 || *(int *)rows[1].address != 9)
        return 1;
    *rows[0].value = 17;
    rows[1].value = &values[1];
    struct row copy = rows[1];
    return values[1] != 17 || *rows[1].value != 17 || *copy.value != 17
        || copy.function() != 13 || *(int *)copy.address != 17
        || copy.head != 3 || copy.tail != 4;
}
