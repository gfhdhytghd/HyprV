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
#include <QTimerEvent>
#include <QtQuick/private/qquickwheelhandler_p_p.h>
#include <QHash>
#include <QJsonDocument>
#include <QJsonArray>
#include <QJsonObject>
#include <QtGui/private/qeventpoint_p.h>
#include <QtQuick/private/qquickflickable_p_p.h>
#include <cstdio>
class Clock:public QAnimationDriver{public:qint64 now=0,base=0;qint64 elapsed()const override{return now-base;}Clock(){connect(this,&QAnimationDriver::started,this,[this]{base=now;});}void step(qint64 t){now=t;advance();}};
int main(int argc,char**argv){QGuiApplication app(argc,argv);Clock clock;clock.install();QVariantAnimation keep;keep.setStartValue(0.);keep.setEndValue(1.);keep.setDuration(10000000);keep.setLoopCount(-1);keep.start();QQuickRenderControl rc;QQuickWindow window(&rc);window.setGeometry(0,0,520,500);QQmlEngine engine;QQmlComponent component(&engine,QUrl::fromLocalFile(QString::fromLocal8Bit(argv[1])));QObject* obj=component.create();if(!obj){qFatal("%s",qPrintable(component.errorString()));}auto* root=qobject_cast<QQuickItem*>(obj);root->setParentItem(window.contentItem());root->setProperty("mode",QString(argv[2]));QCoreApplication::processEvents();rc.polishItems();auto* flick=root->property("flick").value<QObject*>();
fprintf(stderr,"Qt=%s threshold=%d lines=%d decel=%g max=%g bounds=%d movement=%d height=%g content=%g\n",qVersion(),app.styleHints()->startDragDistance(),app.styleHints()->wheelScrollLines(),flick->property("flickDeceleration").toDouble(),flick->property("maximumFlickVelocity").toDouble(),flick->property("boundsBehavior").toInt(),flick->property("boundsMovement").toInt(),flick->property("height").toDouble(),flick->property("contentHeight").toDouble());
QFile file(argv[3]);if(!file.open(QIODevice::ReadOnly))qFatal("cannot open input sequence");auto steps=QJsonDocument::fromJson(file.readAll()).array();Qt::MouseButtons buttons;
qint64 pressTime = -1;
QHash<QQuickWheelHandler*,qint64> wheelDeadlines;
printf("time,y,x,clicks,moving,dragging\n");
const int tick=argc>4?QString(argv[4]).toInt():16;
for(int time=0;time<=3500;time+=tick){clock.step(time);QCoreApplication::processEvents();rc.polishItems();
auto *pd=QQuickFlickablePrivate::get(static_cast<QQuickFlickable*>(flick));
// QQuickFlickable uses a basic timer for pressDelay; deliver that timer at the
// virtual deadline (no real sleeps), using its real Qt timerEvent implementation.
if(pressTime >= 0 && time >= pressTime + flick->property("pressDelay").toInt() && pd->delayedPressEvent) {
 QTimerEvent timeout(pd->delayedPressTimer.timerId()); QCoreApplication::sendEvent(flick,&timeout);
}
for(auto it=wheelDeadlines.begin();it!=wheelDeadlines.end();) {
 if(time >= it.value()) {
  auto *p=QQuickWheelHandlerPrivate::get(it.key());
  if(p->deactivationTimer.isActive()) {QTimerEvent timeout(p->deactivationTimer.timerId());QCoreApplication::sendEvent(it.key(),&timeout);}
  it=wheelDeadlines.erase(it);
 } else ++it;
}
for(auto value:steps){auto s=value.toObject();if(s["t"].toInt()!=time)continue;auto type=s["type"].toString();if(type=="set") {flick->setProperty(s["key"].toString().toUtf8(),s["value"].toVariant());}
else if(type=="wheel"){int ph=s["phase"].toInt();bool pixel=s["pixel"].toBool();QWheelEvent e(QPointF(100,200),QPointF(100,200),QPoint(s["px"].toInt(),s["py"].toInt()),QPoint(s["ax"].toInt(),s["ay"].toInt()),buttons,Qt::NoModifier,Qt::ScrollPhase(ph),false,pixel?Qt::MouseEventSynthesizedBySystem:Qt::MouseEventNotSynthesized);e.setTimestamp(time+1);QCoreApplication::sendEvent(&window,&e);
for(auto *h:flick->findChildren<QQuickWheelHandler*>()) {
 auto *p=QQuickWheelHandlerPrivate::get(h);
 if(ph==0 && p->deactivationTimer.isActive())wheelDeadlines[h]=time+100;
 else if(!h->active())wheelDeadlines.remove(h);
}
}
else {QEvent::Type et= type=="press"?QEvent::MouseButtonPress:type=="release"?QEvent::MouseButtonRelease:QEvent::MouseMove;Qt::MouseButton b=et==QEvent::MouseMove?Qt::NoButton:Qt::LeftButton;if(et==QEvent::MouseButtonPress)buttons|=b;if(et==QEvent::MouseButtonRelease)buttons&=~b;if(et==QEvent::MouseButtonPress)pressTime=time;
QPointF pos(s.contains("x")?s["x"].toDouble():100,s["y"].toDouble());QMouseEvent e(et,pos,pos,pos,b,buttons,Qt::NoModifier);e.setTimestamp(time+1);
// Explicit device-local velocity makes virtual input independent of wall-clock delivery.
// These traces also include zero-velocity moves, and a separate native-estimator case.
if(s.contains("v"))QMutableEventPoint::setVelocity(e.point(0),QVector2D(s["vx"].toDouble(),s["v"].toDouble()));
QCoreApplication::sendEvent(&window,&e);}
QCoreApplication::processEvents();rc.polishItems();}
printf("%d,%.12f,%.12f,%d,%d,%d\n", time, flick->property("contentY").toDouble(), flick->property("contentX").toDouble(),root->property("clicks").toInt(),flick->property("moving").toBool(),flick->property("dragging").toBool());}
root->setParentItem(nullptr);delete root;
}
