// Independent development oracle; never linked to the shell.
#include <QtGui/QGuiApplication>
#include <QtGui/QPainterPath>
#include <QtQml/QQmlEngine>
#include <QtQml/QQmlComponent>
#include <QtQuick/QQuickItem>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtCore/QJsonArray>
#include <cmath>
#include <cstdio>
int main(int argc,char**argv){QGuiApplication app(argc,argv);QJsonArray arcs,centers,quadratics;
 for(auto spec:QList<QList<double>>{{12,12,9,-M_PI/2,-M_PI/2+2*M_PI*.37,0},{25,25,22,0,2*M_PI,0},{12,12,9,-M_PI/2,-M_PI/2,0},{14,14,11,.2,-4.8,1},{12,12,9,4.9,1.3,0}}){
  double xc=spec[0],yc=spec[1],radius=spec[2],sar=spec[3],ear=spec[4];bool antiClockWise=spec[5];QPainterPath p;
  if(sar!=ear){sar=-sar;ear=-ear;antiClockWise=!antiClockWise;float sa=qRadiansToDegrees(sar),ea=qRadiansToDegrees(ear);double span=0;
   if((!antiClockWise&&(ea-sa>=360))||(antiClockWise&&(sa-ea>=360)))span=360;
   else{if(!antiClockWise&&ea<sa)span+=360;else if(antiClockWise&&sa<ea)span-=360;if(!(qFuzzyCompare(span+(ea-sa)+1,1)&&qFuzzyCompare(qAbs(span),360)))span+=ea-sa;}
   p.arcMoveTo(xc-radius,yc-radius,2*radius,2*radius,sa);p.arcTo(xc-radius,yc-radius,2*radius,2*radius,sa,span);
  }
  QJsonArray e;for(int i=0;i<p.elementCount();++i){auto v=p.elementAt(i);e.append(QJsonArray{int(v.type),v.x,v.y});}QJsonArray args;for(auto v:spec)args.append(v);arcs.append(QJsonObject{{"args",args},{"elements",e}});
 }
 for(auto v:QList<QList<double>>{{0,0,1,2,3,4},{2.3,-1.7,7.9,9.4,8.8,3.3},{30,2,30,0,32,0}}){QPainterPath p;p.moveTo(v[0],v[1]);p.quadTo(v[2],v[3],v[4],v[5]);QJsonArray args,e;for(double x:v)args.append(x);for(int i=0;i<p.elementCount();++i){auto a=p.elementAt(i);e.append(QJsonArray{int(a.type),a.x,a.y});}quadratics.append(QJsonObject{{"args",args},{"elements",e}});}
 QQmlEngine engine;for(auto sizes:QList<QList<double>>{{120.16,38.98},{29.3,12.2},{100,21},{101,20},{60.1,61.7}}){QQmlComponent component(&engine);auto src=QString("import QtQuick\nItem {width:%1; height:90.3; Item {objectName:\"child\";width:%2;height:21.3;anchors.centerIn:parent} }").arg(sizes[0],0,'g',16).arg(sizes[1],0,'g',16).toUtf8();component.setData(src,QUrl());QObject*root=component.create();if(!root){qFatal("QML: %s",qPrintable(component.errorString()));}auto child=root->findChild<QQuickItem*>("child");centers.append(QJsonObject{{"parent",sizes[0]},{"child",sizes[1]},{"x",child->x()},{"y",child->y()}});delete root;}
 std::puts(QJsonDocument(QJsonObject{{"qt",qVersion()},{"arcs",arcs},{"centers",centers},{"quadratics",quadratics}}).toJson().constData());
}
