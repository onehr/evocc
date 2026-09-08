extern char **environ;

int main(void) {
    return environ == 0;
}
