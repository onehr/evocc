extern double floating_pressure(double);
void floating_clobber(void) {
    __asm__ volatile(
        "movi v0.16b,#99\n movi v1.16b,#99\n movi v2.16b,#99\n movi v3.16b,#99\n"
        "movi v4.16b,#99\n movi v5.16b,#99\n movi v6.16b,#99\n movi v7.16b,#99\n"
        "movi v16.16b,#99\n movi v17.16b,#99\n movi v18.16b,#99\n movi v19.16b,#99\n"
        "movi v20.16b,#99\n movi v21.16b,#99\n movi v22.16b,#99\n movi v23.16b,#99\n"
        "movi v24.16b,#99\n movi v25.16b,#99\n movi v26.16b,#99\n movi v27.16b,#99\n"
        "movi v28.16b,#99\n movi v29.16b,#99\n movi v30.16b,#99\n movi v31.16b,#99"
        : : : "v0","v1","v2","v3","v4","v5","v6","v7",
        "v16","v17","v18","v19","v20","v21","v22","v23",
        "v24","v25","v26","v27","v28","v29","v30","v31");
}
int main(void) { return floating_pressure(0.25) != 312.0; }
