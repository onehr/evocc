int data;
int ready;

void publish(int value) {
    data = value;
    __sync_synchronize();
    ready = 1;
}

int main(void) {
    _Static_assert(__builtin_types_compatible_p(__typeof__(__sync_synchronize()), void), "void fence");
    publish(7);
    return data != 7 || ready != 1;
}
