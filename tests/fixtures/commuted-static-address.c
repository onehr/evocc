static int values[3] = {7, 11, 19};
static unsigned long first = 0 + (unsigned long)&values[0];
static unsigned long second = sizeof(int) + (unsigned long)&values[0];
static unsigned long last = ((unsigned long)&values[2] - sizeof(int)) + sizeof(int);
static int *element = 2 + values;
struct entry { unsigned long data; };
static const struct entry table[] = {
    {0 + (unsigned long)&values[0]},
    {4 + (unsigned long)&values[0]},
};

int main(void) {
    return *(int *)first != 7 || *(int *)second != 11 || *(int *)last != 19 ||
           *element != 19 || *(int *)table[0].data != 7 || *(int *)table[1].data != 11;
}
