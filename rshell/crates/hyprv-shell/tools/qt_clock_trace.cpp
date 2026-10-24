// Offline QML plugin: logs actual Qt unified-timer state; never linked at runtime.
#include <QtQml/QQmlExtensionPlugin>
#include <QtQml/qqml.h>
#include <QtCore/QDateTime>
#include <QtCore/QElapsedTimer>
#include <QtCore/QAbstractAnimation>
#include <cstdio>
#define private public
#include <QtCore/private/qabstractanimation_p.h>
#undef private
static void sample(qint64 delta) {
    auto *u=QUnifiedTimer::instance(false);
    fprintf(stderr,"CORE_CLOCK wall=%lld elapsed=%lld last=%lld driver_start=%lld drift=%lld time=%lld driver=%d delta=%lld\n",
        (long long)QDateTime::currentMSecsSinceEpoch(), (long long)u->elapsed(), (long long)u->lastTick,
        (long long)u->driverStartTime,(long long)u->temporalDrift,
        (long long)(u->time.isValid()?u->time.elapsed():-1),u->driver->isRunning(),(long long)delta);
}
class CoreTrace:public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QQmlExtensionInterface_iid)
public:
    void registerTypes(const char *uri) override { qmlRegisterModule(uri,1,0); }
    void initializeEngine(QQmlEngine*,const char*) override {
        QUnifiedTimer::instance()->registerProfilerCallback(sample);
    }
};
#include "qt_clock_trace.moc"
