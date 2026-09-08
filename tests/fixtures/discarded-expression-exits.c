int total;

void choose(int c) {
    c ? ({ return; }) : (void)(total += 3);
    total += 5;
}

void both(int c) {
    c ? ({ total += 1; return; }) : ({ total += 2; return; });
    total += 100;
}

void jump(int c) {
    (c ? ({ goto end; }) : (void)(total += 4)), total += 8;
end:
    total += 16;
}

void loop(void) {
    for (int i = 0; i < 5; i++) {
        i == 1 ? ({ continue; }) : (void)(total += 1);
        i == 3 ? ({ break; }) : (void)(total += 2);
    }
}

void initialize(void) {
    for (({ return; }); ; ) total += 100;
    total += 100;
}

void step(void) {
    for (int i = 0; i < 3; ({ return; })) total += 1;
    total += 100;
}

int main(void) {
    choose(1);
    choose(0);
    if (total != 8) return 1;
    both(0);
    both(1);
    if (total != 11) return 2;
    jump(1);
    jump(0);
    if (total != 55) return 3;
    loop();
    if (total != 62) return 4;
    initialize();
    step();
    return total != 63;
}
