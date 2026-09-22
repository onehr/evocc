int choose(int x) { if (x) return; return 7; }
int main(void) { choose(1); return choose(0) != 7; }
