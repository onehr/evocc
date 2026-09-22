#include <dlfcn.h>
#include <stdio.h>
static int trace[10], count;
void event(int value){if(count<10)trace[count++]=value;}
int main(int argc,char **argv){
    if(argc!=2)return 1;
    for(int round=0;round<2;round++){
        void *a=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);
        void *b=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);
        if(!a||!b){fprintf(stderr,"%s\n",dlerror());return 2;}
        if(count!=round*5+2)return 3;
        void (*touch)(void)=dlsym(a,"lifecycle_touch");
        if(!touch)return 4;
        touch();dlclose(a);
        if(count!=round*5+3)return 5;
        dlclose(b);
        if(count!=round*5+5)return 6;
    }
    for(int i=0;i<10;i++)if(trace[i]!=i%5+1)return 7;
    return 0;
}
