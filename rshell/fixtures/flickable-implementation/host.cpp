#include <QAnimationDriver>
#include <QGuiApplication>
#include <QStyleHints>
#include <QQuickRenderControl>
#include <QQuickWindow>
#include <QQuickItem>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QMouseEvent>
#include <QWheelEvent>
#include <QPointingDevice>
#include <QVariantAnimation>
#include <QFile>
#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <QtGui/private/qeventpoint_p.h>
#include <QtQuick/private/qquickflickable_p_p.h>
#include <cstdio>
class Clock:public QAnimationDriver{public:qint64 now=0,base=0;qint64 elapsed()const override{return now-base;}Clock(){connect(this,&QAnimationDriver::started,this,[this]{base=now;});}void step(qint64 t){now=t;advance();}};
int main(int argc,char**argv){QGuiApplication app(argc,argv);Clock clock;clock.install();QVariantAnimation keep;keep.setStartValue(0.);keep.setEndValue(1.);keep.setDuration(10000000);keep.setLoopCount(-1);keep.start();QQuickRenderControl rc;QQuickWindow window(&rc);window.setGeometry(0,0,300,420);QQmlEngine engine;QQmlComponent component(&engine,QUrl::fromLocalFile(QString::fromLocal8Bit(argv[1])));QObject* obj=component.create();if(!obj){qFatal("%s",qPrintable(component.errorString()));}auto* root=qobject_cast<QQuickItem*>(obj);root->setParentItem(window.contentItem());root->setProperty("listMode",QString(argv[2])=="list");QCoreApplication::processEvents();rc.polishItems();auto* flick=root->property("flick").value<QObject*>();
fprintf(stderr,"Qt=%s threshold=%d lines=%d decel=%g max=%g bounds=%d movement=%d height=%g content=%g\n",qVersion(),app.styleHints()->startDragDistance(),app.styleHints()->wheelScrollLines(),flick->property("flickDeceleration").toDouble(),flick->property("maximumFlickVelocity").toDouble(),flick->property("boundsBehavior").toInt(),flick->property("boundsMovement").toInt(),flick->property("height").toDouble(),flick->property("contentHeight").toDouble());
QFile file(argv[3]);if(!file.open(QIODevice::ReadOnly))qFatal("cannot open input sequence");auto steps=QJsonDocument::fromJson(file.readAll()).array();Qt::MouseButtons buttons;
printf("time,y,moving,dragging,velocity,clicks,raw_velocity,overshoot,fixup,timeline,fixupmode\n");
const int tick=argc>4?QString(argv[4]).toInt():16;
for(int time=0;time<=3500;time+=tick){clock.step(time);QCoreApplication::processEvents();rc.polishItems();for(auto value:steps){auto s=value.toObject();if(s["t"].toInt()!=time)continue;auto type=s["type"].toString();if(type=="set") {flick->setProperty(s["key"].toString().toUtf8(),s["value"].toVariant());}
else if(type=="wheel"){int ph=s["phase"].toInt();bool pixel=s["pixel"].toBool();QWheelEvent e(QPointF(100,200),QPointF(100,200),QPoint(0,s["py"].toInt()),QPoint(0,s["ay"].toInt()),buttons,Qt::NoModifier,Qt::ScrollPhase(ph),false,pixel?Qt::MouseEventSynthesizedBySystem:Qt::MouseEventNotSynthesized);e.setTimestamp(time+1);QCoreApplication::sendEvent(&window,&e);}
else {QEvent::Type et= type=="press"?QEvent::MouseButtonPress:type=="release"?QEvent::MouseButtonRelease:QEvent::MouseMove;Qt::MouseButton b=et==QEvent::MouseMove?Qt::NoButton:Qt::LeftButton;if(et==QEvent::MouseButtonPress)buttons|=b;if(et==QEvent::MouseButtonRelease)buttons&=~b;QPointF pos(100,s["y"].toDouble());QMouseEvent e(et,pos,pos,pos,b,buttons,Qt::NoModifier);e.setTimestamp(time+1);
// Explicit device-local velocity makes virtual input independent of wall-clock delivery.
// These traces also include zero-velocity moves, and a separate native-estimator case.
if(s.contains("v"))QMutableEventPoint::setVelocity(e.point(0),QVector2D(0,s["v"].toDouble()));
QCoreApplication::sendEvent(&window,&e);}
QCoreApplication::processEvents();rc.polishItems();}
auto* priv=QQuickFlickablePrivate::get(static_cast<QQuickFlickable*>(flick));
printf("%d,%.12f,%d,%d,%.12f,%d,%.12f,%d,%d,%d,%d\n",time,flick->property("contentY").toDouble(),flick->property("moving").toBool(),flick->property("dragging").toBool(),flick->property("verticalVelocity").toDouble(),root->property("clicks").toInt(),priv->vData.velocity,priv->vData.inOvershoot,priv->vData.fixingUp,priv->timeline.time(),int(priv->fixupMode));}
root->setParentItem(nullptr);delete root;
}
