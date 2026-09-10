typedef char n;

int sized(long n, int data[sizeof(n++)]) {
    return (int)n + data[0];
}

int adjusted(int first[3], int second[sizeof(first)]) {
    return first[0] + second[0];
}

int callback(char n, int data[sizeof(n)]) {
    return n + data[0];
}

int nested(long n, int (*call)(char n, int data[sizeof(n)]),
           int data[sizeof(n)]) {
    return (int)n + call(2, data);
}

int main(void) {
    n unchanged = 7;
    int data[8] = {11};
    if (sizeof(unchanged) != 1) return 1;
    if (sized(5, data) != 16) return 2;
    if (adjusted(data, data) != 22) return 3;
    if (nested(3, callback, data) != 16) return 4;
    return 0;
}
