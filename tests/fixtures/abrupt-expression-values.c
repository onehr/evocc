int effects;
int consume(int value) { effects += 100; return value; }
int assignment(void) {
    int value = 2;
    value = 4 + ({ ++effects; goto done; 9; });
    effects += 1000;
done:
    return value;
}
int returned(void) {
    int value = ({ return 7; 0; });
    return value + 2;
}
int argument(void) {
    int value = consume(({ goto done; 9; }));
    return value;
done:
    return 3;
}
int pointer(void) {
    int value = 5;
    int *p = ({ goto done; &value; });
    return *p;
done:
    return value;
}
int loops(void) {
    while (1) {
        int value = ({ ++effects; break; 0; });
        effects += value + 1000;
    }
    for (int i = 0; i < 3; ++i) {
        int value = ({ continue; 7; });
        effects += value;
    }
    return effects;
}
int main(void) {
    if (assignment() != 2 || effects != 1) return 1;
    if (returned() != 7 || argument() != 3 || effects != 1) return 2;
    if (pointer() != 5 || loops() != 2) return 3;
    return 0;
}
