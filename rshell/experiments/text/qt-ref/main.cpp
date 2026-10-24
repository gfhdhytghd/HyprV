// textref: render a QML scene offscreen and save it as a straight-alpha RGBA PNG.
// Usage: QT_SCALE_FACTOR=2 QT_QPA_PLATFORM=offscreen textref scene.qml out.png
// Also prints the font engine facts Qt resolved for every Text item (fonts.json-ish lines).
#include <QGuiApplication>
#include <QQuickView>
#include <QQuickItem>
#include <QQmlEngine>
#include <QSGRendererInterface>
#include <QImage>
#include <QRawFont>
#include <QTextLayout>
#include <QFontMetricsF>
#include <QFontInfo>
#include <QGlyphRun>
#include <private/qrawfont_p.h>
#include <private/qfontengine_p.h>
#include <cstdio>
#include <private/qhighdpiscaling_p.h>
#include <private/qdistancefield_p.h>
#include <QFile>
#include <QPainterPath>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>

static void dumpFonts(QQuickItem *item)
{
    for (QQuickItem *c : item->childItems()) {
        if (c->inherits("QQuickText")) {
            const QFont f = c->property("font").value<QFont>();
            const QString text = c->property("text").toString();
            QFontMetricsF fm(f);
            QTextLayout layout(text, f);
            layout.beginLayout(); QTextLine line = layout.createLine(); layout.endLayout();
            std::printf("TEXT %s | family=%s px=%d w=%d | implicit=%.3fx%.3f fm.asc=%.3f desc=%.3f height=%.3f leading=%.3f hadv=%.3f\n",
                qPrintable(c->objectName()), qPrintable(f.family()), f.pixelSize(), f.weight(),
                c->property("implicitWidth").toDouble(), c->property("implicitHeight").toDouble(),
                fm.ascent(), fm.descent(), fm.height(), fm.leading(), fm.horizontalAdvance(text));
            for (const QGlyphRun &run : line.glyphRuns()) {
                QRawFont rf = run.rawFont();
                QString pos;
                for (QPointF p : run.positions()) pos += QString::number(p.x(), 'f', 3) + " ";
                if (c->property("renderType").toInt() == 0) {
                    const bool narrow = qt_fontHasNarrowOutlines(rf);
                    std::printf("   df narrow=%d glyphCount=%d base=%d scale=%d radius=%d\n", int(narrow), int(QRawFontPrivate::get(rf)->fontEngine->glyphCount()),
                                QT_DISTANCEFIELD_BASEFONTSIZE(narrow), QT_DISTANCEFIELD_SCALE(narrow), QT_DISTANCEFIELD_RADIUS(narrow));
                }
                std::printf("   run font=%s style=%s px=%.2f hint=%d glyphs=%lld pos=[%s] y=%.3f\n",
                    qPrintable(rf.familyName()), qPrintable(rf.styleName()), rf.pixelSize(), int(rf.hintingPreference()),
                    (long long)run.glyphIndexes().size(), qPrintable(pos), run.positions().isEmpty() ? 0.0 : run.positions().first().y());
            }
        }
        dumpFonts(c);
    }
}

// textref --dumpdf family weight text out.json : Qt's own distance fields (as the glyph cache builds them)
static int dumpDf(const QString &family, int weight, const QString &text, const QString &out)
{
    QFont f(family); f.setWeight(QFont::Weight(weight)); f.setPixelSize(16);
    QTextLayout layout(text, f); layout.beginLayout(); layout.createLine(); layout.endLayout();
    QJsonArray arr;
    for (const QGlyphRun &run : layout.lineAt(0).glyphRuns()) {
        QRawFont rf = run.rawFont();
        const bool narrow = qt_fontHasNarrowOutlines(rf) && QRawFontPrivate::get(rf)->fontEngine->glyphCount() < QT_DISTANCEFIELD_HIGHGLYPHCOUNT();
        QRawFont ref = rf; ref.setPixelSize(QT_DISTANCEFIELD_BASEFONTSIZE(narrow) * QT_DISTANCEFIELD_SCALE(narrow));
        for (quint32 g : run.glyphIndexes()) {
            QPainterPath path = ref.pathForGlyph(g);
            QRectF br = path.boundingRect();
            const int sc = QT_DISTANCEFIELD_SCALE(narrow), margin = QT_DISTANCEFIELD_RADIUS(narrow) / sc;
            QDistanceField df(path, g, narrow);   // same size rule as QDistanceFieldData::create(path)
            QJsonObject o; o["family"] = rf.familyName(); o["glyph"] = int(g); o["narrow"] = narrow;
            o["bx"] = br.x() / sc; o["by"] = br.y() / sc; o["bw"] = br.width() / sc; o["bh"] = br.height() / sc;
            o["margin"] = margin; o["w"] = df.width(); o["h"] = df.height();
            QJsonArray px; for (int y = 0; y < df.height(); ++y) for (int x = 0; x < df.width(); ++x) px.append(int(df.constScanLine(y)[x]));
            o["data"] = px; arr.append(o);
        }
    }
    QFile file(out); file.open(QIODevice::WriteOnly); file.write(QJsonDocument(arr).toJson(QJsonDocument::Compact));
    return 0;
}

int main(int argc, char **argv)
{
    QGuiApplication app(argc, argv);
    if (argc >= 6 && QByteArray(argv[1]) == "--dumpdf")
        return dumpDf(QString::fromUtf8(argv[2]), atoi(argv[3]), QString::fromUtf8(argv[4]), QString::fromLocal8Bit(argv[5]));
    if (argc < 3) { std::fprintf(stderr, "usage: textref scene.qml out.png\n"); return 2; }
    QSurfaceFormat fmt = QSurfaceFormat::defaultFormat(); fmt.setAlphaBufferSize(8); QSurfaceFormat::setDefaultFormat(fmt);
    QQuickView view;
    view.setColor(Qt::transparent);
    view.setResizeMode(QQuickView::SizeViewToRootObject);
    view.setSource(QUrl::fromLocalFile(QString::fromLocal8Bit(argv[1])));
    if (view.status() != QQuickView::Ready) {
        for (auto &e : view.errors()) std::fprintf(stderr, "%s\n", qPrintable(e.toString()));
        return 3;
    }
    if (!qEnvironmentVariableIsSet("TEXTREF_NO_SHOW")) view.show();
    QCoreApplication::processEvents();
    QImage img = view.grabWindow();
    for (int i = 0; i < 4; ++i) { QCoreApplication::processEvents(); QImage again = view.grabWindow(); if (again == img) break; img = again; }
    std::printf("api=%d dpr=%.2f highdpiActive=%d size=%dx%d\n", int(view.rendererInterface()->graphicsApi()),
                view.devicePixelRatio(), int(QHighDpiScaling::isActive()), img.width(), img.height());
    dumpFonts(view.rootObject());
    img = img.convertToFormat(QImage::Format_RGBA8888);
    if (!img.save(QString::fromLocal8Bit(argv[2]))) return 4;
    return 0;
}
