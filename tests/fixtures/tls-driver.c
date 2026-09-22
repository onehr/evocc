#include <pthread.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
static int *(*address_fn)(void);
static int (*read_fn)(void);
static int (*update_fn)(int);
static int (*reference_fn)(void);
static int *(*reference_address_fn)(void);
static pthread_barrier_t ready, release_threads, loaded;
static int *escaped[2];
static int failed[2];
#ifdef TLS_LINKED
extern int *tls_address(void), *tls_reference_address(void);
extern int tls_read(void), tls_update(int), tls_reference(void);
#endif
static void *worker(void *argument) {
    long i=(long)argument;
    pthread_barrier_wait(&loaded);
    int ok=read_fn()==7 && reference_fn()==7 && update_fn(100+(int)i)==18;
    escaped[i]=address_fn();
    ok=ok && escaped[i]==reference_address_fn() && *escaped[i]==100+i;
    pthread_barrier_wait(&ready);
    pthread_barrier_wait(&release_threads);
    ok=ok && read_fn()==200+i && reference_fn()==200+i && update_fn(300+(int)i)==20;
    failed[i]=!ok;
    return 0;
}
int main(int argc,char **argv) {
#ifndef TLS_LINKED
    if(argc!=2)return 1;
#else
    (void)argc; (void)argv;
#endif
    pthread_barrier_init(&ready,0,3);
    pthread_barrier_init(&release_threads,0,3);
    pthread_barrier_init(&loaded,0,3);
    pthread_t a,b;
    pthread_create(&a,0,worker,(void*)0);
    pthread_create(&b,0,worker,(void*)1);
    /* Existing threads must acquire instances of a subsequently loaded DSO. */
#ifdef TLS_LINKED
    address_fn=tls_address;read_fn=tls_read;update_fn=tls_update;
    reference_fn=tls_reference;reference_address_fn=tls_reference_address;
#else
    void *library=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);
    if(!library){fprintf(stderr,"%s\n",dlerror());return 2;}
    address_fn=dlsym(library,"tls_address");read_fn=dlsym(library,"tls_read");
    update_fn=dlsym(library,"tls_update");reference_fn=dlsym(library,"tls_reference");
    reference_address_fn=dlsym(library,"tls_reference_address");
#endif
    if(!address_fn||!read_fn||!update_fn||!reference_fn||!reference_address_fn)return 3;
    pthread_barrier_wait(&loaded);
    pthread_barrier_wait(&ready);
    int ok=read_fn()==7 && *escaped[0]==100 && *escaped[1]==101
        && escaped[0]!=escaped[1] && address_fn()!=escaped[0] && address_fn()!=escaped[1];
    *escaped[0]=200;*escaped[1]=201;
    pthread_barrier_wait(&release_threads);
    pthread_join(a,0);pthread_join(b,0);
    ok=ok && !failed[0] && !failed[1] && read_fn()==7 && update_fn(9)==18;
#ifndef TLS_LINKED
    dlclose(library);
#endif
    return !ok;
}
