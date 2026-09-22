typedef int iv4 __attribute__((vector_size(16)));
typedef unsigned char bv16 __attribute__((vector_size(16)));
typedef short sv4 __attribute__((vector_size(8)));
typedef int iv8 __attribute__((vector_size(32)));
typedef short sv16 __attribute__((vector_size(32)));
typedef unsigned char bv4 __attribute__((vector_size(4)));
static iv4 objects[2]={{1,2,3,4},{5,6,7,8}};
static int calls;
static iv4 *locate(void){++calls;return objects;}
int main(void){
    (*locate()) += (iv4){2,3,4,5};
    if(calls!=1||objects[0][0]!=3||objects[0][3]!=9)return 1;
    iv4 old=objects[0]; objects[0]+=1;
    if(old[0]!=3||objects[0][0]!=4)return 2;
    objects[0]-=1; iv4 b=objects[0];
    b*=2;b-=1;b/=1;b%=16;b^=2;b|=1;b&=15;b<<=1;b>>=1;
    if(b[0]!=7||b[1]!=11||b[2]!=15||b[3]!=3)return 3;
    sv4 s={-1,2,-3,4};iv4 w=__builtin_convertvector(s,iv4);
    if(w[0]!=-1||w[2]!=-3)return 4;
    sv4 back=__builtin_convertvector(w,sv4);
    if(back[0]!=-1||back[3]!=4)return 5;
    __uint128_t packed=(__uint128_t)(iv4){1,2,3,4};
    if((unsigned long)(packed>>64)!=((4UL<<32)|3))return 6;
    iv4 unpacked=(iv4)packed;
    if(unpacked[1]!=2||unpacked[3]!=4)return 7;
    bv16 bytes=(bv16)unpacked;
    if(bytes[0]!=1||bytes[4]!=2||bytes[12]!=4)return 8;
    iv8 wide={1,2,3,4,5,6,7,8};
    wide+=1;
    iv8 reversed=__builtin_shufflevector(wide,wide,7,6,5,4,3,2,1,0);
    if(reversed[0]!=9||reversed[7]!=2)return 9;
    sv16 bits=(sv16)reversed;
    if(bits[0]!=9||bits[1]!=0||bits[14]!=2)return 10;
    bv4 little={255,2,3,4};little+=1;
    if(sizeof(little)!=4||little[0]!=0||little[3]!=5)return 11;
    return 0;
}
