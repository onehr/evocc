struct text { char head; char data[]; } text = { 'o', "wx" };
struct text listed = { 'l', { 'w', 'x', 0 } };
struct text empty = { 'e', "" };
struct wide { int head; unsigned int data[]; } wide = { 7, L"wx" };
char terminated[5] = "wx";
char exact[2] = "wx";
int main(void) {
    if (sizeof(struct text) != 1 || sizeof(text) != 1) return 1;
    if (text.head != 'o' || text.data[0] != 'w' || text.data[1] != 'x' || text.data[2] != 0) return 2;
    if (listed.data[1] != text.data[1] || listed.data[2] != 0 || empty.data[0] != 0) return 3;
    if (sizeof(struct wide) != 4 || wide.head != 7 || wide.data[0] != 'w' || wide.data[1] != 'x' || wide.data[2] != 0) return 4;
    if (terminated[4] != 0 || exact[1] != 'x') return 5;
    text.data[1] = 'z';
    if (text.data[1] != 'z' || listed.data[1] != 'x') return 6;
    return 0;
}
