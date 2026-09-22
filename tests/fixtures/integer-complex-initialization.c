struct object { _Complex int value; int tail; };
int main(void) {
    struct object a = {0};
    _Complex int empty[3] = {};
    if (a.value || a.tail || empty[0] || empty[1] || empty[2]) return 1;
    _Complex short z = 2 + 3i;
    int choose = 0;
    _Complex int selected = choose ? z : 7;
    if (selected != 7) return 2;
    selected = (choose = 1) ? z : 7;
    return selected != 2 + 3i;
}
