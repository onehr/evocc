typedef char n;
enum { width = 2 };

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

int matrix(long width, int (*data)[sizeof(width)]) {
    return sizeof(*data) + data[1][0];
}

int main(void) {
    n unchanged = 7;
    int data[8] = {11};
    int rows[2][8] = {{1}, {7}};
    if (sizeof(unchanged) != 1) return 1;
    if (sized(5, data) != 16) return 2;
    if (adjusted(data, data) != 22) return 3;
    if (nested(3, callback, data) != 16) return 4;
    if (matrix(0, (void *)rows) != 39) return 5;
    return 0;
}
