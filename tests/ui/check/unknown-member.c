// flags: --parse-only
struct record { int value; };
int main(void) { struct record item = { 0 }; return item.missing; }
