int hidden_value = 13;
int default_value = 17;
int hidden_function(void) { return 19; }
extern int counter;
extern int *reference;
int *hidden_address(void);
int *default_address(void);
int call_hidden(void);
int read_counter(void);
int alternate(void);
int main(void) {
    if ((unsigned long)&counter % 64) return 1;
    if (reference != &counter || counter != 0) return 2;
    if (hidden_address() != &hidden_value) return 3;
    if (default_address() != &default_value) return 4;
    if (call_hidden() != 19) return 5;
    counter = 23;
    if (read_counter() != 30) return 6;
    if (alternate() != 29) return 7;
    return 0;
}
