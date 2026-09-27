extern double scalar_asm_transform(double, double, int);
extern float scalar_asm_float(float, float);
extern double scalar_asm_spill(int);
extern float scalar_asm_spillf(int);
int main(void) {
    if (scalar_asm_transform(1.25, 2.5, 0) != 3.75) return 1;
    if (scalar_asm_transform(1.25, 2.5, 1) != -3.75) return 2;
    if (scalar_asm_float(1.25f, 2.5f) != -3.75f) return 3;
    if (scalar_asm_spill(0) != 76.0 || scalar_asm_spill(1) != 78.0) return 4;
    if (scalar_asm_spillf(0) != 76.0f || scalar_asm_spillf(1) != 78.0f) return 5;
    return 0;
}
