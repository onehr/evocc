struct __attribute__((packed)) entry { unsigned char a, b; const char *name; };
static const struct entry entries[] = {{1,2,"abc"}, {3,4,"xyz"}};
int main(void)
{
    return sizeof(entries) != 20 || __builtin_offsetof(struct entry, name) != 2
        || entries[0].name[1] != 'b' || entries[1].name[1] != 'y';
}
