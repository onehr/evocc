extern int puts(const char *);

int (*address(void))(const char *) {
    return puts;
}

int main(void) {
    return address() == 0;
}
