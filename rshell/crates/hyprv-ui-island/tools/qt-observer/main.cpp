#include <private/qquicktext_p_p.h>
#include <private/qquickwindow_p.h>
#include <private/qquickdeliveryagent_p_p.h>
#include <QSGTextureProvider>
#include <QSGTexture>
// parity-host: renders original QML components offscreen with a virtual clock.
//
// One process renders one fixture in one theme at one scale; the scale is the
// device pixel ratio of the render target (as the live fractional-scale DPR).
//
// Determinism:
// - A custom QAnimationDriver reports virtual milliseconds; time only moves
//   when a fixture step says so. The scene keeps one infinite animation
//   running so QML Timers are never moved to Qt's real-time pause timer.
// - The scene is driven by QQuickRenderControl, so nothing renders on its own.
//   Every virtual tick (60 Hz by default) runs one full frame (polish, which
//   also flushes frame-synchronous hover events, then sync + render), exactly
//   like one vsync of the live basic render loop.

#include <QAnimationDriver>
#include <QCommandLineParser>
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QFontDatabase>
#include <QFontInfo>
#include <QApplication>
#include <QGuiApplication>
#include <QIcon>
#include <QPalette>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLoggingCategory>
#include <QMouseEvent>
#include <QPointingDevice>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickRenderControl>
#include <QQuickRenderTarget>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QWheelEvent>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <memory>
#include <rhi/qrhi.h>
#include <dlfcn.h>
#include <sys/time.h>
#include <time.h>

#include "wayland_mocks.h"

// Virtual wall clock. The QML global object (and Date) is frozen, so instead
// the host interposes the libc wall-clock functions: CLOCK_REALTIME reads as
// fixture "now" + virtual animation time. Monotonic clocks stay real, so Qt's
// timers and event loop are unaffected. Covers Date.now(), new Date(),
// QDateTime::currentDateTime() and everything else that asks for wall time.
namespace parityclock {
double g_baseMs = 0;
const double *g_virtualMs = nullptr;
bool g_enabled = false;
inline qint64 nowMs() { return qint64(g_baseMs + (g_virtualMs ? *g_virtualMs : 0.0)); }
}

extern "C" int clock_gettime(clockid_t id, struct timespec *ts)
{
    using Fn = int (*)(clockid_t, struct timespec *);
    static Fn real = reinterpret_cast<Fn>(dlsym(RTLD_NEXT, "clock_gettime"));
    if (parityclock::g_enabled && (id == CLOCK_REALTIME || id == CLOCK_REALTIME_COARSE) && ts) {
        const qint64 ms = parityclock::nowMs();
        ts->tv_sec = ms / 1000;
        ts->tv_nsec = (ms % 1000) * 1000000;
        return 0;
    }
    return real(id, ts);
}

extern "C" int gettimeofday(struct timeval *tv, void *tz)
{
    using Fn = int (*)(struct timeval *, void *);
    static Fn real = reinterpret_cast<Fn>(dlsym(RTLD_NEXT, "gettimeofday"));
    if (parityclock::g_enabled && tv) {
        const qint64 ms = parityclock::nowMs();
        tv->tv_sec = ms / 1000;
        tv->tv_usec = (ms % 1000) * 1000;
        return 0;
    }
    return real(tv, tz);
}

extern "C" time_t time(time_t *out)
{
    using Fn = time_t (*)(time_t *);
    static Fn real = reinterpret_cast<Fn>(dlsym(RTLD_NEXT, "time"));
    if (!parityclock::g_enabled)
        return real(out);
    const time_t t = time_t(parityclock::nowMs() / 1000);
    if (out)
        *out = t;
    return t;
}

namespace {

[[noreturn]] void die(const QString &message, int code = 2)
{
    std::fprintf(stderr, "parity-host: %s\n", qPrintable(message));
    std::exit(code);
}

class VirtualDriver : public QAnimationDriver
{
public:
    VirtualDriver()
    {
        connect(this, &QAnimationDriver::started, this, [this] { m_startedAt = m_now; });
    }

    qint64 elapsed() const override { return qint64(std::floor(m_now)) - qint64(std::floor(m_startedAt)); }

    double now() const { return m_now; }
    const double *nowPtr() const { return &m_now; }

    void stepTo(double t)
    {
        m_now = t;
        advance();
    }

private:
    double m_now = 0;
    double m_startedAt = 0;
};

struct Run
{
    QQuickRenderControl *control = nullptr;
    QQuickWindow *window = nullptr;
    QQuickItem *root = nullptr;
    VirtualDriver *driver = nullptr;
    QRhi *rhi = nullptr;
    std::unique_ptr<QRhiTexture> texture;
    std::unique_ptr<QRhiRenderBuffer> depthStencil;
    std::unique_ptr<QRhiTextureRenderTarget> target;
    std::unique_ptr<QRhiRenderPassDescriptor> pass;
    QSize pixelSize;
    double tickMs = 1000.0 / 60.0;
    double dpr = 1.0;
    QString outDir;
    QString component;
    QString state;
    QString theme;
    QString scaleTag;
    QJsonArray manifest;
    QRectF crop; // logical; empty = whole canvas
    QPointF mouse{-1, -1};
    Qt::MouseButtons buttons;
};

void ensureTarget(Run &run)
{
    const QSize size = (QSizeF(run.window->size()) * run.dpr).toSize();
    if (run.texture && size == run.pixelSize)
        return;
    run.pixelSize = size;
    run.target.reset();
    run.pass.reset();
    run.texture.reset(run.rhi->newTexture(QRhiTexture::RGBA8, size, 1,
                                          QRhiTexture::RenderTarget | QRhiTexture::UsedAsTransferSource));
    if (!run.texture->create())
        die("cannot create render texture");
    run.depthStencil.reset(run.rhi->newRenderBuffer(QRhiRenderBuffer::DepthStencil, size, 1));
    if (!run.depthStencil->create())
        die("cannot create depth-stencil buffer");
    QRhiTextureRenderTargetDescription desc{QRhiColorAttachment(run.texture.get())};
    desc.setDepthStencilBuffer(run.depthStencil.get());
    run.target.reset(run.rhi->newTextureRenderTarget(desc));
    run.pass.reset(run.target->newCompatibleRenderPassDescriptor());
    run.target->setRenderPassDescriptor(run.pass.get());
    if (!run.target->create())
        die("cannot create render target");
    QQuickRenderTarget target = QQuickRenderTarget::fromRhiRenderTarget(run.target.get());
    target.setDevicePixelRatio(run.dpr);
    run.window->setRenderTarget(target);
}

// One frame of the live render loop. Returns the image when readback is set.
QImage frame(Run &run, bool readback)
{
    if(qEnvironmentVariableIsSet("HYPRV_GL_TRACE_FILE")) qputenv("HYPRV_GL_TRACE_TICK",QByteArray::number(run.driver->now(),'g',17));
    QCoreApplication::sendPostedEvents();
    ensureTarget(run);
    if(qEnvironmentVariableIsSet("HYPRV_RADIUS_TRACE")) {
        auto d=QQuickWindowPrivate::get(run.window)->deliveryAgentPrivate();
        fprintf(stderr,"HOVER_TICK t=%.17g pos%.17g,%.17g enabled%d dirty%d\n",run.driver->now(),d->lastMousePosition.x(),d->lastMousePosition.y(),d->frameSynchronousHoverInterval,QQuickWindowPrivate::get(run.window)->dirtyItemList!=nullptr);
    }
    if(qEnvironmentVariableIsSet("HYPRV_RADIUS_TRACE")) {
        std::function<void(QQuickItem*)> scan=[&](QQuickItem *item){
            if(item->inherits("QQuickMouseArea") && item->property("hoverEnabled").toBool())
                fprintf(stderr,"MOUSE_TICK t%.17g x%.17g y%.17g w%.17g h%.17g visible%d enabled%d contains%d\n",run.driver->now(),item->mapToScene(QPointF()).x(),item->mapToScene(QPointF()).y(),item->width(),item->height(),item->isVisible(),item->isEnabled(),item->property("containsMouse").toBool());
            for(auto c:item->childItems())scan(c);
        };scan(run.window->contentItem());
    }
    run.control->polishItems();
    run.control->beginFrame();
    run.control->sync();
    run.control->render();
    QRhiReadbackResult result;
    if (readback) {
        QRhiResourceUpdateBatch *batch = run.rhi->nextResourceUpdateBatch();
        batch->readBackTexture(QRhiReadbackDescription(run.texture.get()), &result);
        run.control->commandBuffer()->resourceUpdate(batch);
    }
    run.control->endFrame();
    if(qEnvironmentVariableIsSet("HYPRV_RADIUS_TRACE")) {
        std::function<void(QQuickItem*)> scan=[&](QQuickItem *item){
            if(item->inherits("QQuickRectangle") && item->width()>90 && item->height()>38 && item->property("radius").toDouble()!=24.)
                fprintf(stderr,"RADIUS_TICK t=%.17g w=%.17g h=%.17g radius=%.17g opacity=%.17g\n",run.driver->now(),item->width(),item->height(),item->property("radius").toDouble(),item->opacity());
            for(auto c:item->childItems())scan(c);
        };scan(run.window->contentItem());
    }
    if (!readback)
        return {};
    QImage image(reinterpret_cast<const uchar *>(result.data.constData()), result.pixelSize.width(),
                 result.pixelSize.height(), QImage::Format_RGBA8888_Premultiplied);
    image = image.copy();
    if (run.rhi->isYUpInFramebuffer())
        image = image.flipped(Qt::Vertical);
    return image;
}

void waitForImages(Run &run)
{
    // Asynchronous Image loads finish in real time; wait for them without
    // moving the virtual clock.
    QElapsedTimer guard;
    guard.start();
    for (;;) {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 5);
        bool loading = false;
        const auto items = run.root->findChildren<QQuickItem *>();
        for (QQuickItem *item : items) {
            // QQuickImageBase::Status::Loading == 2
            if (item->inherits("QQuickImageBase") && item->property("status").toInt() == 2)
                loading = true;
        }
        if (!loading)
            return;
        if (guard.elapsed() > 5000)
            die("timed out waiting for images to load");
    }
}

void advance(Run &run, double ms)
{
    const double target = run.driver->now() + ms;
    while (run.driver->now() + 1e-9 < target) {
        const double next = std::min(target, run.driver->now() + run.tickMs);
        run.driver->stepTo(next);
        QCoreApplication::processEvents(QEventLoop::AllEvents);
        frame(run, false);
    }
}

QPointF pointFor(Run &run, const QJsonObject &step, const QString &key)
{
    const QJsonValue value = step.value(key);
    QPointF p;
    QQuickItem *base = run.root;
    if (value.isArray()) {
        p = QPointF(value[0].toDouble(), value[1].toDouble());
    } else if (value.isObject() && (value.toObject().contains("text") || value.toObject().contains("contains")
                                    || value.toObject().contains("image"))) {
        // {"text": "label", "index": n, "x": dx, "y": dy} or {"image": "substring", ...}:
        // the n-th (scene order: top-to-bottom, left-to-right) effectively visible
        // Text with exactly that text / Image whose source contains the substring;
        // the point is its centre plus (dx, dy).
        const QJsonObject o = value.toObject();
        // "contains": substring match on Text instead of exact.
        const bool partial = o.contains("contains");
        const bool byText = o.contains("text") || partial;
        const QString needle = partial ? o.value("contains").toString()
                                       : (byText ? o.value("text").toString() : o.value("image").toString());
        // Walk the visual tree: Repeater/Instantiator delegates have no QObject
        // parent, so findChildren() would miss them.
        QList<QQuickItem *> hits;
        std::function<void(QQuickItem *)> walk = [&](QQuickItem *item) {
            if (!item->isVisible() || item->opacity() <= 0.01)
                return;
            bool match = false;
            if (byText && item->inherits("QQuickText"))
                match = partial ? item->property("text").toString().contains(needle)
                                : item->property("text").toString() == needle;
            else if (!byText && item->inherits("QQuickImageBase"))
                match = item->property("source").toUrl().toString().contains(needle);
            if (match)
                hits.append(item);
            for (QQuickItem *child : item->childItems())
                walk(child);
        };
        walk(run.window->contentItem());
        std::sort(hits.begin(), hits.end(), [](QQuickItem *a, QQuickItem *b) {
            const QPointF pa = a->mapToScene(QPointF()), pb = b->mapToScene(QPointF());
            return pa.y() != pb.y() ? pa.y() < pb.y() : pa.x() < pb.x();
        });
        const int index = o.value("index").toInt(0);
        if (index >= hits.size())
            die(QStringLiteral("no visible %1 matching '%2' (index %3, found %4)").arg(byText ? "Text" : "Image", needle).arg(index).arg(hits.size()));
        QQuickItem *hit = hits.at(index);
        const QPointF c = hit->mapToScene(QPointF(hit->width() / 2 + o.value("x").toDouble(), hit->height() / 2 + o.value("y").toDouble()));
        return QPointF(std::floor(c.x()), std::floor(c.y()));
    } else if (value.isObject()) {
        const QJsonObject o = value.toObject();
        const QString name = o.value("item").toString();
        if (!name.isEmpty()) {
            base = run.root->findChild<QQuickItem *>(name);
            if (!base)
                die("no item with objectName " + name);
        }
        p = QPointF(o.value("x").toDouble(), o.value("y").toDouble());
    } else {
        die("step '" + key + "' needs [x,y] or {item,x,y}");
    }
    // Integer logical positions, like a pointer on a scale-1 grid.
    const QPointF scene = base->mapToScene(p);
    return QPointF(std::floor(scene.x()), std::floor(scene.y()));
}

void sendMouse(Run &run, QEvent::Type type, QPointF pos, Qt::MouseButton button)
{
    if (type == QEvent::MouseButtonPress)
        run.buttons |= button;
    else if (type == QEvent::MouseButtonRelease)
        run.buttons &= ~button;
    QMouseEvent event(type, pos, pos, pos, button, run.buttons, Qt::NoModifier, QPointingDevice::primaryPointingDevice());
    QCoreApplication::sendEvent(run.window, &event);
    QCoreApplication::processEvents();
    run.mouse = pos;
}

void moveMouse(Run &run, QPointF to, bool path = false)
{
    // Default: jump straight to the target (hover enter fires for whatever is
    // under it). With "path": true, move in 4-px hops so every item crossed on
    // the way sees enter/leave, like a real pointer sweep.
    const QPointF from = run.mouse.x() < 0 ? to : run.mouse;
    const int hops = path ? std::max(1, int(std::ceil(std::hypot(to.x() - from.x(), to.y() - from.y()) / 4.0))) : 1;
    for (int i = 1; i <= hops; ++i)
        sendMouse(run, QEvent::MouseMove, from + (to - from) * (double(i) / hops), Qt::NoButton);
}

void capture(Run &run, const QString &name, double t)
{
    waitForImages(run);
    // Layers and effects (MultiEffect mask) pick up a newly loaded texture on
    // the next sync; render (without advancing time) until stable.
    QImage image = frame(run, true);
    // Diagnostic: observe original one-readback capture ordering.
    if (!qEnvironmentVariableIsSet("HYPRV_SINGLE_FRAME")) {
    for (int i = 0; i < 6; ++i) {
        QImage again = frame(run, true);
        if (again == image)
            break;
        image = again;
    }
    }
    if (image.isNull())
        die("render returned a null image");
    if (!run.crop.isEmpty()) {
        const QRect pixels(qRound(run.crop.x() * run.dpr), qRound(run.crop.y() * run.dpr),
                           qRound(run.crop.width() * run.dpr), qRound(run.crop.height() * run.dpr));
        image = image.copy(pixels);
    }

    QString file = run.state;
    if (!name.isEmpty())
        file += "--" + name;
    file += "--" + run.theme + "@" + run.scaleTag + ".png";
    // PNG holds straight (non-premultiplied) alpha; the README documents it.
    const QImage straight = image.convertToFormat(QImage::Format_RGBA8888);
    if (qEnvironmentVariableIsSet("PARITY_RAW_PM")) {
        // Exact premultiplied RGBA8 framebuffer bytes (no unpremultiply rounding), for
        // measuring conversion error: <file>.pm.rgba, row-major, width*4 stride.
        const QImage pm = image.convertToFormat(QImage::Format_RGBA8888_Premultiplied);
        QFile raw(QDir(run.outDir).filePath(QString(file).replace(QStringLiteral(".png"), QStringLiteral(".pm.rgba"))));
        if (raw.open(QIODevice::WriteOnly))
            for (int y = 0; y < pm.height(); ++y)
                raw.write(reinterpret_cast<const char *>(pm.constScanLine(y)), pm.width() * 4);
    }
    if (!straight.save(QDir(run.outDir).filePath(file)))
        die("failed to write " + file);

    QJsonObject entry;
    entry["file"] = file;
    entry["state"] = run.state;
    entry["frame"] = name;
    entry["theme"] = run.theme;
    entry["scale"] = run.scaleTag.toDouble();
    entry["virtual_ms"] = t;
    // Distance-field text (Text.QtRendering, enum 0) has +-1 run-to-run GPU
    // noise; flag it so the diff can apply its tolerance only where needed.
    bool distanceField = false;
    std::function<void(QQuickItem *)> scan = [&](QQuickItem *item) {
        if (distanceField || !item->isVisible() || item->opacity() <= 0.01)
            return;
        if (item->inherits("QQuickText") && item->property("renderType").toInt() == 0
            && !item->property("text").toString().isEmpty()) {
            const QRectF area = run.crop.isEmpty() ? QRectF(0, 0, run.window->width(), run.window->height()) : run.crop;
            if (item->mapRectToScene(QRectF(0, 0, item->width(), item->height())).intersects(area))
                distanceField = true;
        }
        for (QQuickItem *child : item->childItems())
            scan(child);
    };
    scan(run.window->contentItem());
    entry["distance_field_text"] = distanceField;
    entry["width_px"] = straight.width();
    entry["height_px"] = straight.height();
    entry["width_logical"] = run.crop.isEmpty() ? run.window->width() : run.crop.width();
    entry["height_logical"] = run.crop.isEmpty() ? run.window->height() : run.crop.height();
    if (!run.crop.isEmpty())
        entry["crop_logical"] = QJsonArray{run.crop.x(), run.crop.y(), run.crop.width(), run.crop.height()};
    run.manifest.append(entry);

    // Finite property observation in owned test host; original QML is untouched.
    std::function<void(QQuickItem *)> observe = [&](QQuickItem *item) {
        if (item->inherits("QQuickText") && item->property("elide").toInt() != 0) {
            auto d = static_cast<QQuickTextPrivate *>(QQuickItemPrivate::get(item));
            auto layout = d->elideLayout ? d->elideLayout.data() : &d->layout;
            fprintf(stderr,"ISLAND_ELIDE %s text=%s elided=%s width=%.17g\n",qPrintable(name),qPrintable(d->text),qPrintable(layout->text()),item->width());
            fprintf(stderr,"ELIDE_ITEM xy %.17g %.17g height%.17g px%d\n",item->mapToScene(QPointF()).x(),item->mapToScene(QPointF()).y(),item->height(),d->font.pixelSize());
            for (auto run : layout->glyphRuns()) {
                fprintf(stderr,"ELIDE_FONT %s asc%.17g desc%.17g px%.17g\n",qPrintable(run.rawFont().familyName()),run.rawFont().ascent(),run.rawFont().descent(),run.rawFont().pixelSize());
                auto ids=run.glyphIndexes(); auto positions=run.positions();
                for(int i=0;i<ids.size();++i) fprintf(stderr,"ELIDE_GLYPH %u %.17g %.17g\n",ids[i],positions[i].x(),positions[i].y());
            }
        }
        if (item->property("glyph").isValid())
            fprintf(stderr,"ISLAND_GLYPH %s text=%s scale=%.17g x=%.9g y=%.9g\n",qPrintable(name),qPrintable(item->property("glyph").toString()),item->scale(),item->mapToScene(QPointF()).x(),item->mapToScene(QPointF()).y());
        if (item->inherits("QQuickRectangle") && item->width()>=90 && item->height()>=38)
            fprintf(stderr,"ISLAND_FRAME %s x=%.17g y=%.17g w=%.17g h=%.17g radius=%.17g opacity=%.17g visible=%d\n",qPrintable(name),item->mapToScene(QPointF()).x(),item->mapToScene(QPointF()).y(),item->width(),item->height(),item->property("radius").toDouble(),item->opacity(),item->isVisible());
        if (item->inherits("QQuickRectangle") && qAbs(item->height()-5.) < .001)
            fprintf(stderr,"ISLAND_RECT %s t=%.9g x=%.9g y=%.9g w=%.17g h=%.9g color=%s\n",qPrintable(name),t,item->mapToScene(QPointF()).x(),item->mapToScene(QPointF()).y(),item->width(),item->height(),qPrintable(item->property("color").value<QColor>().name(QColor::HexArgb)));
        if (item->inherits("QQuickCanvasItem") && item->isVisible() && item->parentItem() && item->parentItem()->property("samples").isValid()) {
            auto chart=item->parentItem();
            fprintf(stderr,"CHART_SAMPLES max=%.17g samples=%s\n",chart->property("resolvedMaxValue").toDouble(),QJsonDocument(QJsonArray::fromVariantList(chart->property("samples").toList())).toJson(QJsonDocument::Compact).constData());
        }
        if (item->inherits("QQuickCanvasItem") && item->isVisible() && qEnvironmentVariableIsSet("HYPRV_CANVAS_SOURCE_DIR")) {
            static int seq=0;
            auto provider=item->textureProvider();
            if(provider && provider->texture() && provider->texture()->rhiTexture()) {
                QRhiCommandBuffer *cb=nullptr;
                run.rhi->beginOffscreenFrame(&cb);
                QRhiReadbackResult result;auto batch=run.rhi->nextResourceUpdateBatch();
                batch->readBackTexture(QRhiReadbackDescription(provider->texture()->rhiTexture()),&result);
                cb->resourceUpdate(batch);run.rhi->endOffscreenFrame();
                if(!result.data.isEmpty()) {
                    QImage raw(reinterpret_cast<const uchar *>(result.data.constData()),result.pixelSize.width(),result.pixelSize.height(),QImage::Format_RGBA8888_Premultiplied);
                    const auto rect=provider->texture()->normalizedTextureSubRect();
                    fprintf(stderr,"CANVAS_SOURCE index=%d format=%d rect=%.17g,%.17g,%.17g,%.17g size=%dx%d\n",seq,int(provider->texture()->rhiTexture()->format()),rect.x(),rect.y(),rect.width(),rect.height(),result.pixelSize.width(),result.pixelSize.height());
                    const QRect sub(qRound(rect.x()*result.pixelSize.width()),qRound(rect.y()*result.pixelSize.height()),qRound(rect.width()*result.pixelSize.width()),qRound(rect.height()*result.pixelSize.height()));
                    raw.copy(sub).save(qEnvironmentVariable("HYPRV_CANVAS_SOURCE_DIR")+QString("/crop-%1.png").arg(seq));
                    raw.save(qEnvironmentVariable("HYPRV_CANVAS_SOURCE_DIR")+QString("/source-%1-%2x%3.png").arg(seq++).arg(result.pixelSize.width()).arg(result.pixelSize.height()));
                }
            }
        }
        if (item->inherits("QQuickCanvasItem"))
            fprintf(stderr,"ISLAND_CANVAS %s t=%.9g phase=%.17g active=%d visible=%d\n",qPrintable(name),t,item->property("phase").toDouble(),item->property("active").toBool(),item->isVisible());
        for (auto child:item->childItems()) observe(child);
    };
    observe(run.window->contentItem());
    if (run.root->property("calendarProgress").isValid()) {
        auto center=run.root->findChild<QQuickItem *>(QStringLiteral("centerSection"));
        fprintf(stderr,"ISLAND_HOST %s progress=%.17g cw=%.17g ch=%.17g radius=%.17g\n",qPrintable(name),run.root->property("calendarProgress").toDouble(),center?center->width():0.,center?center->height():0.,center?center->property("radius").toDouble():0.);
    }

    // Regions sidecar (regions/<file>.json): every visible Text / Image / Canvas
    // intersecting the capture, in logical px relative to the capture origin.
    // Lets the verifier attribute diff pixels to text, icons or shapes.
    {
        const QRectF area = run.crop.isEmpty() ? QRectF(0, 0, run.window->width(), run.window->height()) : run.crop;
        QJsonArray regions;
        std::function<void(QQuickItem *)> collect = [&](QQuickItem *item) {
            if (!item->isVisible() || item->opacity() <= 0.01)
                return;
            QString kind;
            if (item->inherits("QQuickText") && !item->property("text").toString().isEmpty())
                kind = QStringLiteral("text");
            else if (item->inherits("QQuickImageBase"))
                kind = QStringLiteral("image");
            else if (item->inherits("QQuickCanvasItem"))
                kind = QStringLiteral("canvas");
            if (!kind.isEmpty()) {
                const QRectF r = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
                if (r.intersects(area) && r.width() > 0 && r.height() > 0) {
                    QJsonObject o;
                    o["kind"] = kind;
                    o["rect"] = QJsonArray{r.x() - area.x(), r.y() - area.y(), r.width(), r.height()};
                    if (kind == QLatin1String("text")) {
                        o["text"] = item->property("text").toString();
                        o["render_type"] = item->property("renderType").toInt() == 0 ? "qt" : "native";
                        const QFont font = item->property("font").value<QFont>();
                        o["font"] = QStringLiteral("%1 %2px w%3").arg(font.family()).arg(font.pixelSize()).arg(font.weight());
                    } else if (kind == QLatin1String("image")) {
                        o["source"] = item->property("source").toUrl().toString();
                    }
                    regions.append(o);
                }
            }
            for (QQuickItem *child : item->childItems())
                collect(child);
        };
        collect(run.window->contentItem());
        QDir(run.outDir).mkpath(QStringLiteral("regions"));
        QFile sidecar(QDir(run.outDir).filePath(QStringLiteral("regions/") + QString(file).replace(QStringLiteral(".png"), QStringLiteral(".json"))));
        if (sidecar.open(QIODevice::WriteOnly))
            sidecar.write(QJsonDocument(regions).toJson(QJsonDocument::Compact));
    }
    std::printf("  %s\n", qPrintable(file));
}

// Authoring aid (PARITY_DUMP_TEXT=1, or step {"dump": true}): list every
// visible Text/Image with its scene rect, to pick pointer targets.
void dumpItems(Run &run)
{
    std::function<void(QQuickItem *)> walk = [&](QQuickItem *item) {
        if (!item->isVisible() || item->opacity() <= 0.01)
            return;
        const bool text = item->inherits("QQuickText");
        const bool image = item->inherits("QQuickImageBase");
        if (text || image) {
            const QRectF r = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
            const QString what = text ? item->property("text").toString() : item->property("source").toUrl().fileName();
            std::fprintf(stderr, "dump: %-5s %7.1f %7.1f %6.1f x %5.1f  %s\n", text ? "text" : "image",
                         r.x(), r.y(), r.width(), r.height(), qPrintable(what.left(60)));
        }
        for (QQuickItem *child : item->childItems())
            walk(child);
    };
    walk(run.window->contentItem());
}

void runSteps(Run &run, const QJsonArray &steps)
{
    for (const QJsonValue &v : steps) {
        const QJsonObject step = v.toObject();
        if (step.contains("set")) {
            QMetaObject::invokeMethod(run.root, "applyProps", Q_ARG(QVariant, step.value("set").toVariant()));
            QCoreApplication::processEvents();
        }
        if (step.contains("invoke")) {
            // {"invoke": {"type": "ControlPanelPopup", "method": "toggleMihomoExpanded", "args": []}}:
            // call a function on the first instance of a QML component (for
            // states the UI cannot reach, see docs/spec/ui-panels-control.md).
            const QJsonObject o = step.value("invoke").toObject();
            const QString type = o.value("type").toString() + QStringLiteral("_QMLTYPE");
            QObject *target = nullptr;
            for (QObject *candidate : run.root->findChildren<QObject *>()) {
                if (QString::fromLatin1(candidate->metaObject()->className()).startsWith(type)) {
                    target = candidate;
                    break;
                }
            }
            if (!target)
                die("invoke: no instance of " + o.value("type").toString());
            const QVariantList args = o.value("args").toArray().toVariantList();
            const QByteArray method = o.value("method").toString().toLatin1();
            bool ok = false;
            switch (args.size()) {
            case 0: ok = QMetaObject::invokeMethod(target, method.constData()); break;
            case 1: ok = QMetaObject::invokeMethod(target, method.constData(), Q_ARG(QVariant, args[0])); break;
            case 2: ok = QMetaObject::invokeMethod(target, method.constData(), Q_ARG(QVariant, args[0]), Q_ARG(QVariant, args[1])); break;
            default: die("invoke: at most 2 args");
            }
            if (!ok)
                die("invoke: method failed: " + o.value("method").toString());
            QCoreApplication::processEvents();
        }
        if (step.contains("hover"))
            moveMouse(run, pointFor(run, step, "hover"), step.value("path").toBool(false));
        if (step.contains("press")) {
            const QPointF p = pointFor(run, step, "press");
            moveMouse(run, p);
            sendMouse(run, QEvent::MouseButtonPress, p, Qt::LeftButton);
        }
        if (step.contains("click")) {
            const QPointF p = pointFor(run, step, "click");
            moveMouse(run, p);
            sendMouse(run, QEvent::MouseButtonPress, p, Qt::LeftButton);
            sendMouse(run, QEvent::MouseButtonRelease, p, Qt::LeftButton);
        }
        if (step.contains("rightclick")) {
            const QPointF p = pointFor(run, step, "rightclick");
            moveMouse(run, p);
            sendMouse(run, QEvent::MouseButtonPress, p, Qt::RightButton);
            sendMouse(run, QEvent::MouseButtonRelease, p, Qt::RightButton);
        }
        if (step.contains("drag")) {
            // Button held: 12 moves, one virtual frame apart.
            const QPointF to = pointFor(run, step, "drag");
            const QPointF from = run.mouse;
            for (int i = 1; i <= 12; ++i) {
                sendMouse(run, QEvent::MouseMove, from + (to - from) * (i / 12.0), Qt::NoButton);
                advance(run, run.tickMs);
            }
        }
        if (step.contains("wheel")) {
            // {"wheel": {"item":..,"x":..,"y":..,"dy": -120}} angle delta, 120 = one notch.
            const QPointF p = pointFor(run, step, "wheel");
            moveMouse(run, p);
            const int dy = step.value("wheel").toObject().value("dy").toInt(-120);
            QWheelEvent wheel(p, p, QPoint(), QPoint(0, dy), run.buttons, Qt::NoModifier, Qt::NoScrollPhase, false,
                              Qt::MouseEventNotSynthesized, QPointingDevice::primaryPointingDevice());
            QCoreApplication::sendEvent(run.window, &wheel);
            QCoreApplication::processEvents();
        }
        if (step.contains("release")) {
            const QPointF p = step.value("release").isBool() ? run.mouse : pointFor(run, step, "release");
            if (p != run.mouse)
                sendMouse(run, QEvent::MouseMove, p, Qt::NoButton);
            sendMouse(run, QEvent::MouseButtonRelease, p, Qt::LeftButton);
        }
        if (step.contains("advance"))
            advance(run, step.value("advance").toDouble());
        if (step.value("dump").toBool() || (step.contains("capture") && qEnvironmentVariableIsSet("PARITY_DUMP_TEXT")))
            dumpItems(run);
        if (step.contains("capture"))
            capture(run, step.value("capture").toString(), run.driver->now());
        if (step.contains("frames")) {
            // {"frames": {"name": "expand", "at": [0, 30, 70]}}: capture at
            // offsets (ms) relative to now; 0 = the instant of the change.
            const QJsonObject f = step.value("frames").toObject();
            const double start = run.driver->now();
            for (const QJsonValue &at : f.value("at").toArray()) {
                const double target = start + at.toDouble();
                if (target > run.driver->now())
                    advance(run, target - run.driver->now());
                capture(run, QStringLiteral("%1-t%2").arg(f.value("name").toString()).arg(int(at.toDouble()), 4, 10, QLatin1Char('0')),
                        run.driver->now() - start);
            }
        }
    }
}

void requireFonts(const QJsonArray &families)
{
    const QStringList available = QFontDatabase::families();
    for (const QJsonValue &f : families) {
        // Qt may list a localized family name (e.g. 鸿蒙黑体 for HarmonyOS
        // Sans SC); accept an exact match through fontconfig aliases too.
        if (!available.contains(f.toString()) && !QFontInfo(QFont(f.toString())).exactMatch())
            die("required font family missing: " + f.toString() + " (refusing to render references)", 3);
    }
}

} // namespace

// Quickshell creates a QApplication when the entry file starts with
// "//@ pragma UseQApplication" (shell.qml does; StartupConfirm.qml does not).
// The platform theme (qt6ct palette + icon theme) only applies in a
// QApplication, so the host mirrors that choice. The island scene renders a
// component of shell.qml, so it counts as shell.qml.
static bool entryWantsQApplication(int argc, char **argv)
{
    QString fixturePath, source;
    for (int i = 1; i + 1 < argc; ++i) {
        if (qstrcmp(argv[i], "--fixture") == 0)
            fixturePath = QString::fromLocal8Bit(argv[i + 1]);
        else if (qstrcmp(argv[i], "--source") == 0)
            source = QString::fromLocal8Bit(argv[i + 1]);
    }
    QFile fixtureFile(fixturePath);
    if (!fixtureFile.open(QIODevice::ReadOnly))
        return true;
    const QString entry = QJsonDocument::fromJson(fixtureFile.readAll()).object().value("entry").toString(QStringLiteral("shell.qml"));
    QFile entryFile(source + QLatin1Char('/') + entry);
    if (!entryFile.open(QIODevice::ReadOnly))
        return true;
    for (int i = 0; i < 5 && !entryFile.atEnd(); ++i) {
        if (entryFile.readLine().trimmed() == "//@ pragma UseQApplication")
            return true;
    }
    return false;
}

int main(int argc, char **argv)
{
    std::unique_ptr<QGuiApplication> application;
    if (entryWantsQApplication(argc, argv))
        application = std::make_unique<QApplication>(argc, argv);
    else
        application = std::make_unique<QGuiApplication>(argc, argv);
    QGuiApplication &app = *application;
    // Always show QML warnings/console output, whatever the user's
    // qtlogging.ini says (it silences js/qml debug output on this machine).
    QLoggingCategory::setFilterRules(QStringLiteral("js.*=true\nqml.*=true\ndefault.warning=true\n") + qEnvironmentVariable("QT_LOGGING_RULES"));

    QCommandLineParser parser;
    parser.addHelpOption();
    parser.addOptions({
        {"fixture", "Fixture JSON file.", "path"},
        {"scenes", "Scene directory.", "path"},
        {"source", "Original quickshell directory (read-only).", "path"},
        {"assets", "Fixture asset directory.", "path"},
        {"out", "Output directory.", "path"},
        {"theme", "light or dark.", "theme"},
        {"scale", "Device pixel ratio to render at (output scale, may be fractional).", "scale"},
        {"tick-ms", "Virtual frame interval in ms (default 1000/60).", "ms"},
    });
    parser.process(app);

    const QString fixturePath = parser.value("fixture");
    QFile fixtureFile(fixturePath);
    if (!fixtureFile.open(QIODevice::ReadOnly))
        die("cannot read fixture " + fixturePath);
    QJsonParseError error;
    const QJsonDocument doc = QJsonDocument::fromJson(fixtureFile.readAll(), &error);
    if (doc.isNull())
        die("fixture parse error in " + fixturePath + ": " + error.errorString());
    const QJsonObject fixture = doc.object();

    requireFonts(fixture.value("fonts").toArray(QJsonArray{"JetBrainsMono Nerd Font"}));

    auto *driver = new VirtualDriver;
    driver->install();
    {
        const QString nowText = fixture.value("now").toString(QStringLiteral("2026-10-23T09:41:00"));
        const QDateTime base = QDateTime::fromString(nowText, Qt::ISODate);
        if (!base.isValid())
            die("fixture 'now' is not an ISO date: " + nowText);
        parityclock::g_baseMs = double(base.toMSecsSinceEpoch());
        parityclock::g_virtualMs = driver->nowPtr();
        parityclock::g_enabled = true;
    }

    Run run;
    run.driver = driver;
    run.outDir = parser.value("out");
    run.component = fixture.value("component").toString();
    run.state = QFileInfo(fixturePath).completeBaseName();
    run.theme = parser.value("theme");
    run.scaleTag = parser.value("scale");
    run.dpr = run.scaleTag.toDouble();
    if (!(run.dpr > 0))
        die("--scale must be > 0");
    if (fixture.contains("crop")) {
        const QJsonArray c = fixture.value("crop").toArray();
        run.crop = QRectF(c[0].toDouble(), c[1].toDouble(), c[2].toDouble(), c[3].toDouble());
    }
    if (fixture.contains("tick_ms"))
        run.tickMs = fixture.value("tick_ms").toDouble();
    if (parser.isSet("tick-ms"))
        run.tickMs = parser.value("tick-ms").toDouble();
    QDir().mkpath(run.outDir);

    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
    QQuickRenderControl control;
    QQuickWindow window(&control);
    run.control = &control;
    run.window = &window;
    window.setColor(Qt::transparent);
    if (!control.initialize())
        die("QQuickRenderControl::initialize failed (no GL; is the headless compositor running?)", 5);
    run.rhi = control.rhi();
    if (!run.rhi)
        die("no QRhi after initialize", 5);
    // References must come from Qt's hardware RHI scenegraph (distance-field
    // and fractionally positioned text differ by up to 38 % in the software
    // renderer, docs/spec/text-rendering.md §7).
    if (run.rhi->backend() != QRhi::OpenGLES2)
        die(QStringLiteral("unexpected RHI backend %1; refusing to render references").arg(run.rhi->backendName()), 5);
    if (qEnvironmentVariableIsSet("PARITY_DEBUG"))
        std::fprintf(stderr, "parity-host: rhi %s device '%s' dpr %.3f palette.window %s icons %s\n", run.rhi->backendName(),
                     run.rhi->driverInfo().deviceName.constData(), run.dpr,
                     qPrintable(QGuiApplication::palette().color(QPalette::Window).name()), qPrintable(QIcon::themeName()));

    registerWaylandMocks();
    QQmlEngine engine;


    engine.addImportPath(QDir(parser.value("scenes")).absoluteFilePath("../qml-mocks"));
    QVariantMap harness;
    harness["theme"] = run.theme;
    harness["dark"] = run.theme == "dark";
    harness["source"] = QDir(parser.value("source")).absolutePath();
    harness["assets"] = QDir(parser.value("assets")).absolutePath();
    harness["fixture"] = fixture.toVariantMap();
    QObject::connect(&engine, &QQmlEngine::exit, &app, [](int code) { die(QStringLiteral("scene requested exit (%1)").arg(code), 4); });
    harness["home"] = QDir::homePath();
    harness["scale"] = run.dpr;
    engine.rootContext()->setContextProperty("Harness", harness);
    LayershellInspector inspector;
    engine.rootContext()->setContextProperty("ParityLayershell", &inspector);
    engine.addImageProvider(QStringLiteral("icon"), new ThemeIconProvider);

    const QString scene = QDir(parser.value("scenes")).absoluteFilePath(fixture.value("scene").toString());
    QQmlComponent component(&engine, QUrl::fromLocalFile(scene));
    QObject *object = component.create();
    run.root = qobject_cast<QQuickItem *>(object);
    if (!run.root) {
        for (const auto &e : component.errors())
            std::fprintf(stderr, "%s\n", qPrintable(e.toString()));
        die("scene failed to load: " + scene);
    }
    run.root->setParentItem(window.contentItem());
    window.setGeometry(0, 0, int(std::ceil(run.root->width())), int(std::ceil(run.root->height())));
    window.contentItem()->setSize(window.size());

    // Initial props, then let every Behavior/Timer settle before the steps.
    QMetaObject::invokeMethod(run.root, "applyProps", Q_ARG(QVariant, fixture.value("props").toVariant()));
    frame(run, false);
    // Canvas, layers and Image sourceSize read the window's effective DPR,
    // text reads the render target's; both must equal the requested scale.
    if (std::abs(window.effectiveDevicePixelRatio() - run.dpr) > 1e-6)
        die(QStringLiteral("effective device pixel ratio is %1, expected %2").arg(window.effectiveDevicePixelRatio()).arg(run.dpr), 5);
    advance(run, fixture.value("settle_ms").toDouble(1500));
    if (qEnvironmentVariableIsSet("PARITY_DEBUG"))
        std::fprintf(stderr, "parity-host: after settle palette.window %s icons %s platformtheme %s\n",
                     qPrintable(QGuiApplication::palette().color(QPalette::Window).name()), qPrintable(QIcon::themeName()),
                     qPrintable(qEnvironmentVariable("QT_QPA_PLATFORMTHEME")));
    // Pointer starts in the canvas corner (outside every island state).
    moveMouse(run, QPointF(window.width() - 1, window.height() - 1));
    advance(run, run.tickMs);

    std::printf("%s/%s [%s @%s]\n", qPrintable(run.component), qPrintable(run.state), qPrintable(run.theme), qPrintable(run.scaleTag));
    runSteps(run, fixture.value("steps").toArray());

    QFile manifest(QDir(run.outDir).filePath(QStringLiteral(".%1--%2@%3.json").arg(run.state, run.theme, run.scaleTag)));
    if (!manifest.open(QIODevice::WriteOnly))
        die("cannot write manifest fragment");
    manifest.write(QJsonDocument(run.manifest).toJson(QJsonDocument::Compact));

    // Release GPU resources before the RHI goes away.
    window.setRenderTarget(QQuickRenderTarget());
    run.target.reset();
    run.pass.reset();
    run.depthStencil.reset();
    run.texture.reset();
    delete object;
    return 0;
}
