struct S { int value; };
static unsigned long size(int n) {
    unsigned long result = sizeof(struct S[n++]);
    return result + n;
}
int main(void) {
    int calls = 0;
    int values[({ ++calls; 4; })];
    values[3] = 9;
    return size(3) != 16 || size(5) != 26
        || calls != 1 || sizeof values != 16 || values[3] != 9;
}
