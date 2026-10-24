// Independent installed-QImage single-pixel and narrow-image conversion oracles.
#include <QImage>
#include <QFile>
int main(int argc, char **argv) {
    if (argc != 3) return 2;
    QByteArray data(256 * 256, '\0');
    for (int alpha = 0; alpha < 256; ++alpha) {
        for (int value = 0; value < 256; ++value) {
            QImage image(1, 1, QImage::Format_ARGB32_Premultiplied);
            image.setPixel(0, 0, qRgba(value, value, value, alpha));
            const QImage out = image.convertToFormat(QImage::Format_RGBA8888);
            data[alpha * 256 + value] = char(out.constBits()[0]);
        }
    }
    QByteArray shapes;
    for (int width = 1; width <= 9; ++width) for (int height = 1; height <= 5; ++height) {
        QImage image(width, height, QImage::Format_ARGB32_Premultiplied);
        image.fill(qRgba(1, 2, 3, 10));
        const QImage out = image.convertToFormat(QImage::Format_RGBA8888);
        shapes.append(char(width)); shapes.append(char(height));
        for (int y = 0; y < height; ++y)
            shapes.append(reinterpret_cast<const char *>(out.constScanLine(y)), width * 4);
    }
    QFile matrix(argv[2]);
    if (!matrix.open(QIODevice::WriteOnly) || matrix.write(shapes) != shapes.size()) return 4;
    QFile file(argv[1]);
    return file.open(QIODevice::WriteOnly) && file.write(data) == data.size() ? 0 : 3;
}
