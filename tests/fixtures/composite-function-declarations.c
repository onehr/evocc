int complete(int);
int complete();
int complete(int value) { return value + 1; }

int first();
int first(int);
int first(int value) { return value + 2; }

typedef int function();
int typed(int);
extern function typed;
int typed(int value) { return value + 3; }

int empty(void);
int empty() { return 4; }

int main(void) {
    if (complete(41) != 42) return 1;
    if (first(40) != 42) return 2;
    if (typed(39) != 42) return 3;
    if (empty() != 4) return 4;
    return 0;
}
