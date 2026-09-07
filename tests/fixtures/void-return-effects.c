int counter;
void sink(int n) { counter += n; }
void direct(void) { return sink(1); }
void indirect(void) { void (*p)(int) = sink; return p(2); }
void casted(void) { return (void)(counter += 4); }
void sequence(void) { return (sink(8), sink(16)); }
void choose(int c) { return c ? sink(32) : sink(64); }
void statement(void) { return ({ sink(128); }); }
int main(void) {
    direct();
    indirect();
    casted();
    sequence();
    choose(1);
    choose(0);
    statement();
    return counter != 255;
}
