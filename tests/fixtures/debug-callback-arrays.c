typedef unsigned long argument_size;
typedef _Bool ready_flag;
typedef struct { void *address; ready_flag kernel; } argument_pointer;
typedef int row_type[5];
struct callback_metadata {
    int (*send)(argument_pointer pointer, argument_size count);
    ready_flag (*ready)(argument_size count);
    unsigned long matrix[4][5];
    row_type named_matrix[2];
    argument_size sizes[2][3];
    unsigned __int128 wide;
    unsigned present:3;
    unsigned :5;
    unsigned next:1;
};
struct callback_metadata metadata;
argument_size describe(argument_size size) { return size; }
int main(void) { return metadata.matrix[3][4] != 0 || describe(7) != 7; }
