/* This scene is rendered by a C program compiled by the EvoCC-built TinyCC. */
#include <math.h>
#include <stdio.h>

typedef struct { double x, y, z; } V;
static V add(V a, V b) { return (V){a.x+b.x,a.y+b.y,a.z+b.z}; }
static V mul(V a, double b) { return (V){a.x*b,a.y*b,a.z*b}; }
static double dot(V a, V b) { return a.x*b.x+a.y*b.y+a.z*b.z; }
static V unit(V a) { return mul(a,1/sqrt(dot(a,a))); }
static V shade(V o, V d, int depth)
{
    V centers[]={{-1.4,0.85,0},{0.6,0.7,-0.25},{2.1,1.1,1.6}};
    V colors[]={{0.1,0.75,0.85},{1,0.24,0.16},{0.75,0.48,0.95}};
    double radii[]={0.85,0.7,1.1}, distance=1e9;
    int hit=-1;
    if(d.y < -1e-6) { distance=-o.y/d.y; hit=3; }
    for(int i=0;i<3;++i) {
        V oc=add(o,mul(centers[i],-1));
        double b=dot(oc,d), disc=b*b-dot(oc,oc)+radii[i]*radii[i];
        if(disc>0) {
            double t=-b-sqrt(disc);
            if(t>0.001 && t<distance) { distance=t; hit=i; }
        }
    }
    if(hit<0) return add(mul((V){0.08,0.13,0.28},1-d.y),mul((V){0.35,0.65,0.85},d.y));
    V p=add(o,mul(d,distance)), n={0,1,0}, color;
    if(hit==3) {
        int tile=((int)floor(p.x)+(int)floor(p.z))&1;
        color=tile?(V){0.28,0.36,0.46}:(V){0.68,0.76,0.80};
    } else { n=unit(add(p,mul(centers[hit],-1))); color=colors[hit]; }
    V light=unit((V){-0.7,1,-0.5});
    double diffuse=fmax(0,dot(n,light));
    double spec=pow(fmax(0,dot(unit(add(light,mul(d,-1))),n)),50);
    color=add(mul(color,0.18+0.72*diffuse),mul((V){1,1,1},0.7*spec));
    if(depth>0) {
        V reflected=shade(add(p,mul(n,0.002)),add(d,mul(n,-2*dot(d,n))),depth-1);
        color=add(mul(color,0.75),mul(reflected,0.25));
    }
    return color;
}
int main(void)
{
    const int w=192,h=108;
    printf("{\"width\":%d,\"height\":%d,\"rgb\":\"",w,h);
    for(int y=0;y<h;++y) for(int x=0;x<w;++x) {
        V c=shade((V){0,2.1,-6},unit((V){(x-w/2.0)/h,(h/2.0-y)/h-0.18,1}),2);
        double a[]={c.x,c.y,c.z};
        for(int i=0;i<3;++i) printf("%02x",(unsigned)(255*sqrt(fmax(0,fmin(1,a[i])))));
    }
    puts("\"}");
    return 0;
}
