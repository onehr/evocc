/* Floating storage, but only byte representations and addresses are used. */
static int storage;
static int *integer_pointer = &storage;
static float *representation_pointer = (float *)&storage;

__attribute__((optimize("-fno-strict-aliasing")))
static void clear_representation(void) { *representation_pointer = 0; }

static int distinct(double *a, double *b) { return a != b; }

int main(void) {
    *integer_pointer = 1;
    clear_representation();
    if (*integer_pointer != 0) return 1;
    struct { int used[3]; double array[1][2]; } objects[2][2];
    float cells[2][2][2][2];
    for (int i = 0; i < 2; ++i)
        for (int j = 0; j < 2; ++j) {
            if (!distinct(objects[0][i].array[0], objects[1][j].array[0]))
                return 2;
            cells[0][i][1][j] = 1000.0;
            unsigned bits;
            __builtin_memcpy(&bits, &cells[0][i][1][j], sizeof bits);
            if (bits != 0x447a0000U) return 3;
        }
    return 0;
}
