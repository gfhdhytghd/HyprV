// Offline QML plugin: read the original bar's RGBA8 before compositor/matte capture.
#include <QtQml/QQmlExtensionPlugin>
#include <QtQml/qqml.h>
#include <QtGui/QGuiApplication>
#include <QtQuick/QQuickWindow>
#include <QtGui/QOpenGLContext>
#include <QtGui/QOpenGLFunctions>
#include <QtGui/QImage>
#include <QtCore/QDir>
#include <QtCore/QSet>
#include <QtCore/QCryptographicHash>
#include <cstdio>
#ifndef CORE_CAPTURE_DIR
#error CORE_CAPTURE_DIR must name a private diagnostic output directory
#endif
class SurfaceTrace:public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QQmlExtensionInterface_iid)
    QSet<QObject*> seen;
public:
    void registerTypes(const char *uri) override { qmlRegisterModule(uri,1,0); }
    void initializeEngine(QQmlEngine*,const char*) override { fprintf(stderr,"CORE_SOURCE init\n"); auto *watcher=new SurfaceTrace; watcher->setParent(qApp); qApp->installEventFilter(watcher); }
    bool eventFilter(QObject *object,QEvent*) override {
        (void)object;
        for (auto *window: QGuiApplication::allWindows()) {
        auto *w=qobject_cast<QQuickWindow*>(window);
        if(w && !seen.contains(w)) {
            fprintf(stderr,"CORE_SOURCE connect %s %dx%d\n", w->metaObject()->className(), w->width(),w->height());
            seen.insert(w);
            connect(w,&QObject::destroyed,this,[this,w]{seen.remove(w);});
            connect(w,&QQuickWindow::afterRendering,w,[w]{
                fprintf(stderr,"CORE_SOURCE frame %dx%d context=%p\n",w->width(),w->height(),QOpenGLContext::currentContext());
                if(w->width()!=3072 || w->height()!=58) return;
                w->beginExternalCommands();
                auto *context=QOpenGLContext::currentContext();if(!context){w->endExternalCommands();return;}
                auto *gl=context->functions();GLint vp[4],fbo=0;
                gl->glGetIntegerv(GL_VIEWPORT,vp);gl->glGetIntegerv(GL_FRAMEBUFFER_BINDING,&fbo);
                if(vp[2]!=6144 || vp[3]!=116) {w->endExternalCommands();return;}
                QByteArray data(vp[2]*vp[3]*4,Qt::Uninitialized);
                gl->glReadPixels(0,0,vp[2],vp[3],GL_RGBA,GL_UNSIGNED_BYTE,data.data());
                auto error=gl->glGetError();w->endExternalCommands();if(error){fprintf(stderr,"CORE_SOURCE error=%x\n",error);return;}
                QDir().mkpath(CORE_CAPTURE_DIR);
                const auto key=QCryptographicHash::hash(data,QCryptographicHash::Sha256).toHex();
                QImage((const uchar*)data.constData(),vp[2],vp[3],vp[2]*4,QImage::Format_RGBA8888_Premultiplied)
                    .flipped(Qt::Vertical).save(QString(CORE_CAPTURE_DIR)+"/bar-"+QString::fromLatin1(key)+".png");
                fprintf(stderr,"CORE_SOURCE fbo=%d size=%dx%d\n",fbo,vp[2],vp[3]);
            },Qt::DirectConnection);
        }
        }
        return false;
    }
};
#include "qt_surface_trace.moc"
