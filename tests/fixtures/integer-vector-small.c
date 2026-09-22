typedef unsigned char v1 __attribute__((vector_size(1)));
typedef unsigned char v2 __attribute__((vector_size(2)));
typedef unsigned char v4 __attribute__((vector_size(4)));
v1 small1(v1 x){return x+1;}
v2 small2(v2 x){return x+1;}
v4 small4(v4 x){return x+1;}
