// Independent Qt 6.12 development oracle. Never linked by the shell.
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QJsonArray>
#include <QtCore/QJsonDocument>
#include <QtCore/QJsonObject>
#include <QtGui/QGuiApplication>
#include <QtGui/QPainter>
#include <QtGui/QPainterPath>
int main(int argc, char **argv) {
  QGuiApplication app(argc, argv);
  QDir dir(argv[1]);
  dir.mkpath(".");
  QJsonArray cases;
  for (double dpr : {1., 1.25, 1.5, 2.})
    for (int shape = 0; shape < 7; shape++)
      for (int cap = 0; cap < 3; cap++) {
        QPainterPath path;
        if (shape == 0 || shape == 6) {
          path.arcMoveTo(QRectF(10.3, 9.7, 38, 38), 90);
          path.arcTo(QRectF(10.3, 9.7, 38, 38), 90, -137.5);
        }
        if (shape == 1) {
          path.addEllipse(QRectF(10, 10, 40, 40));
        }
        if (shape == 2) {
          path.moveTo(8.3, 49.1);
          path.lineTo(27.7, 11.2);
          path.lineTo(50.6, 43.8);
        }
        if (shape == 3) {
          path.moveTo(7.5, 21.7);
          path.lineTo(55.3, 40.2);
        }
        if (shape == 4) {
          path.moveTo(8, 40);
          path.cubicTo(16, 4, 37, 57, 55, 11);
        }
        if (shape == 5) {
          path.moveTo(10, 10);
          path.lineTo(50, 10);
        }
        double width = shape == 5 ? 1. : shape == 3 ? 3.25 : 2.25;
        QImage image(qRound(64 * dpr), qRound(64 * dpr),
                     QImage::Format_ARGB32_Premultiplied);
        image.fill(Qt::transparent);
        {
          QPainter p(&image);
          p.setRenderHint(QPainter::Antialiasing);
          p.scale(dpr, dpr);
          QPen pen(QColor::fromRgbF(.2, .4, .8, .7), width, Qt::SolidLine,
                   cap == 0   ? Qt::FlatCap
                   : cap == 1 ? Qt::SquareCap
                              : Qt::RoundCap,
                   Qt::RoundJoin);
          pen.setMiterLimit(2);
          p.setPen(pen);
          if (shape == 6) {
            QPainterPath track; track.addEllipse(QRectF(10.3, 9.7, 38, 38));
            p.setPen(QPen(QColor::fromRgbF(.6,.3,.1,.35), width, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
            p.drawPath(track);
            p.setPen(pen);
          }
          p.drawPath(path);
        }
        QString id = QString("stroke-%1-%2@%3").arg(shape).arg(cap).arg(dpr);
        image.save(dir.filePath(id + ".png"));
        QJsonArray elements;
        for (int i = 0; i < path.elementCount(); i++) {
          auto e = path.elementAt(i);
          elements.append(QJsonArray{int(e.type), e.x, e.y});
        }
        cases.append(QJsonObject{{"id", id}, {"batch", shape == 6},
                                 {"dpr", dpr},
                                 {"width", width},
                                 {"cap", cap},
                                 {"elements", elements}});
      }
  QFile f(dir.filePath("cases.json"));
  f.open(QIODevice::WriteOnly);
  f.write(QJsonDocument(QJsonObject{{"qt", qVersion()}, {"cases", cases}})
              .toJson());
}
