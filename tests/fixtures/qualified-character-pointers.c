static char *returned(const char *p) { return p; }
static int passed(char *p) { return p[0]; }
static void projected(char **p, const char *source) { *p = source + 1; }

int main(void)
{
    char bytes[4] = "abc";
    const char *view = bytes;
    char *initialized = view;
    char *assigned = 0;
    assigned = view;
    char *result = returned(view);
    if (initialized != bytes || assigned != bytes || result != bytes
        || passed(view) != 'a')
        return 1;
    projected(&assigned, view);
    if (assigned != bytes + 1 || *assigned != 'b')
        return 2;
    *result = 'z';
    return bytes[0] != 'z';
}
