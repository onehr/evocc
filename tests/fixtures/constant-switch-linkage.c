extern void __bad_size_call_parameter(void);

int main(void) {
    unsigned int variable = 0;
    unsigned long value = ({
        unsigned long result;
        switch (sizeof(variable)) {
        case 1: result = 1; break;
        case 2: result = 2; break;
        case 4: result = 4; break;
        case 8: result = 8; break;
        default: __bad_size_call_parameter(); break;
        }
        result;
    });
    return value != 4;
}
