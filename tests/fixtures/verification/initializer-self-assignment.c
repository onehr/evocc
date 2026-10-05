/* C11 6.5.17 and 6.7.9: the comma sequences the inner write before the
   initializer's resulting value. The object is mutable, not re-declared. */
int initializer_self_assignment(void) {
    int x = (x = 1, 2);
    return x;
}

int main(void) {
    return initializer_self_assignment() != 2;
}
