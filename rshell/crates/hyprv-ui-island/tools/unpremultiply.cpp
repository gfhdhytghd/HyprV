// Independent Qt QImage format-conversion oracle, not linked into Rust runtime.
#include <QImage>
#include <QFile>
#include <cstdio>
int main(int argc,char**argv){if(argc!=2)return 2;QImage image(256,256,QImage::Format_ARGB32_Premultiplied);for(int a=0;a<256;++a)for(int p=0;p<256;++p)image.setPixel(p,a,qRgba(p,p,p,a));const QImage out=image.convertToFormat(QImage::Format_RGBA8888);QFile f(argv[1]);if(!f.open(QIODevice::WriteOnly))return 3;for(int a=0;a<256;++a)for(int p=0;p<256;++p){const char value=out.constScanLine(a)[p*4];f.write(&value,1);}fprintf(stderr,"Qt %s QImage ARGB32PM→RGBA8888 256x256 SIMD oracle\n",qVersion());}
