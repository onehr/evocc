struct empty {};
static struct empty objects[4];
static char rows[3][0];
int main(void)
{
    int count = 0;
    struct empty *p = objects;
    p += ++count;
    p++;
    p -= 2;
    --p;
    if (p != objects || count != 1 || &objects[3] != objects)
        return 1;
    char (*q)[0] = rows;
    q += ++count;
    if (q != rows || &rows[2] != rows || count != 2)
        return 2;
    return 0;
}
