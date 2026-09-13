static int replacement = 7;
int *choose(void) { return &replacement; }

int read_choice(void);
int **choose_pair(void);
int *choose_qualified(void);
int *alternative(void);
extern int *pointer_object;
extern int (*callback)(void);

int main(void) {
    return read_choice() != 7 || **choose_pair() != 3 ||
        *choose_qualified() != 3 || *alternative() != 3 ||
        pointer_object != 0 || callback != 0;
}
