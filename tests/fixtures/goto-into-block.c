int result;
int initializers;
int initialize(void) { initializers++; return 7; }
int main(void) {
    int *p = &result;
    if (1) goto target;
    else {
        unsigned char temporary = initialize();
target:
        *p = ((temporary = 4), temporary);
    }
    if (result != 4 || initializers) return 1;
    goto later;
    int skipped = initialize();
later:
    skipped = 9;
    return skipped != 9 || initializers;
}
