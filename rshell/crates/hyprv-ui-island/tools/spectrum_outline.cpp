#include <QImage>
#include <QPainter>
#include <QPainterPath>
#include <private/qoutlinemapper_p.h>
#include <cmath>
#include <cstdio>
int main(int argc,char **argv){
 for(int active=0;active<2;++active){double ratio=active ? std::max(.18,std::min(1.,std::pow(.12,.62)*2.05)) : .18+.34*std::pow((std::sin(6.44)+1)/2,1.45);ratio=.18+(std::min(1.,ratio)-.18)*.65;
 const double w=2,h=std::max(3.,22*.98*ratio),x=0,y=11-h/2,r=1;
 QPainterPath p;p.moveTo(x+r,y);p.lineTo(x+w-r,y);p.quadTo(x+w,y,x+w,y+r);p.lineTo(x+w,y+h-r);p.quadTo(x+w,y+h,x+w-r,y+h);p.lineTo(x+r,y+h);p.quadTo(x,y+h,x,y+h-r);p.lineTo(x,y+r);p.quadTo(x,y,x+r,y);p.closeSubpath();

 QOutlineMapper m;m.setMatrix(QTransform::fromScale(2,2));m.setClipRect(QRect(0,0,44,44));auto outline=m.convertPath(p);printf("ACTIVE%d height%.17g\n",active,h);for(int i=0;i<outline->n_points;++i)printf("%ld,%ld\n",outline->points[i].x,outline->points[i].y);
 QImage im(44,44,QImage::Format_ARGB32_Premultiplied);im.fill(Qt::transparent);QPainter painter(&im);painter.setRenderHint(QPainter::Antialiasing);painter.scale(2,2);painter.fillPath(p,Qt::white);painter.end();im.save(QString::fromLocal8Bit(argv[1])+QString("/firstbar-%1.png").arg(active));
 }
}
