static int evaluations;
static int bound(int n) { ++evaluations; return n; }
static int inspect(int n, void *data) {
    int loops=0;
    goto entry;
entry:
    if (((int (*)[bound(n)])data)[1][2]!=6) return 1;
    if (++loops<2) goto entry;
    if (evaluations!=2) return 2;
    switch(n) {
    case 3:
        if (((int (*)[bound(n)])data)[1][1]!=5) return 3;
        break;
    default:
        return 4;
    }
    return evaluations!=3;
}
int main(void) {
    int data[6]={1,2,3,4,5,6};
    return inspect(3,data);
}
