// Copyright (C) 2016 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Qt 6.12 qdistancefield.cpp numerical kernels; standalone standard C++ storage.
#include "qt_compat.h"
namespace
{
    enum FillHDir
    {
        LeftToRight,
        RightToLeft
    };

    enum FillVDir
    {
        TopDown,
        BottomUp
    };

    enum FillClip
    {
        NoClip,
        Clip
    };
}

template <FillClip clip, FillHDir dir>
inline void fillLine(qint32 *, int, int, int, qint32, qint32)
{
}

template <>
inline void fillLine<Clip, LeftToRight>(qint32 *line, int width, int lx, int rx, qint32 d, qint32 dd)
{
    int fromX = qMax(0, lx >> 8);
    int toX = qMin(width, rx >> 8);
    int x = toX - fromX;
    if (x <= 0)
        return;
    qint32 val = d + (((fromX << 8) + 0xff - lx) * dd >> 8);
    line += fromX;
    do {
        *line = abs(val) < abs(*line) ? val : *line;
        val += dd;
        ++line;
    } while (--x);
}

template <>
inline void fillLine<Clip, RightToLeft>(qint32 *line, int width, int lx, int rx, qint32 d, qint32 dd)
{
    int fromX = qMax(0, lx >> 8);
    int toX = qMin(width, rx >> 8);
    int x = toX - fromX;
    if (x <= 0)
        return;
    qint32 val = d + (((toX << 8) + 0xff - rx) * dd >> 8);
    line += toX;
    do {
        val -= dd;
        --line;
        *line = abs(val) < abs(*line) ? val : *line;
    } while (--x);
}

template <>
inline void fillLine<NoClip, LeftToRight>(qint32 *line, int, int lx, int rx, qint32 d, qint32 dd)
{
    int fromX = lx >> 8;
    int toX = rx >> 8;
    int x = toX - fromX;
    if (x <= 0)
        return;
    qint32 val = d + ((~lx & 0xff) * dd >> 8);
    line += fromX;
    do {
        *line = abs(val) < abs(*line) ? val : *line;
        val += dd;
        ++line;
    } while (--x);
}

template <>
inline void fillLine<NoClip, RightToLeft>(qint32 *line, int, int lx, int rx, qint32 d, qint32 dd)
{
    int fromX = lx >> 8;
    int toX = rx >> 8;
    int x = toX - fromX;
    if (x <= 0)
        return;
    qint32 val = d + ((~rx & 0xff) * dd >> 8);
    line += toX;
    do {
        val -= dd;
        --line;
        *line = abs(val) < abs(*line) ? val : *line;
    } while (--x);
}

template <FillClip clip, FillVDir vDir, FillHDir hDir>
inline void fillLines(qint32 *bits, int width, int height, int upperY, int lowerY,
                      int &lx, int ldx, int &rx, int rdx, qint32 &d, qint32 ddy, qint32 ddx)
{
    Q_UNUSED(height);
    Q_ASSERT(upperY < lowerY);
    int y = lowerY - upperY;
    if (vDir == TopDown) {
        qint32 *line = bits + upperY * width;
        do {
            fillLine<clip, hDir>(line, width, lx, rx, d, ddx);
            lx += ldx;
            d += ddy;
            rx += rdx;
            line += width;
        } while (--y);
    } else {
        qint32 *line = bits + lowerY * width;
        do {
            lx -= ldx;
            d -= ddy;
            rx -= rdx;
            line -= width;
            fillLine<clip, hDir>(line, width, lx, rx, d, ddx);
        } while (--y);
    }
}

template <FillClip clip>
void drawTriangle(qint32 *bits, int width, int height, const QPoint *center,
                  const QPoint *v1, const QPoint *v2, qint32 value)
{
    const int y1 = clip == Clip ? qBound(0, v1->y() >> 8, height) : v1->y() >> 8;
    const int y2 = clip == Clip ? qBound(0, v2->y() >> 8, height) : v2->y() >> 8;
    const int yC = clip == Clip ? qBound(0, center->y() >> 8, height) : center->y() >> 8;

    const int v1Frac = clip == Clip ? (y1 << 8) + 0xff - v1->y() : ~v1->y() & 0xff;
    const int v2Frac = clip == Clip ? (y2 << 8) + 0xff - v2->y() : ~v2->y() & 0xff;
    const int centerFrac = clip == Clip ? (yC << 8) + 0xff - center->y() : ~center->y() & 0xff;

    int dx1 = 0, x1 = 0, dx2 = 0, x2 = 0;
    qint32 dd1, d1, dd2, d2;
    if (v1->y() != center->y()) {
        dx1 = ((v1->x() - center->x()) << 8) / (v1->y() - center->y());
        x1 = center->x() + centerFrac * (v1->x() - center->x()) / (v1->y() - center->y());
    }
    if (v2->y() != center->y()) {
        dx2 = ((v2->x() - center->x()) << 8) / (v2->y() - center->y());
        x2 = center->x() + centerFrac * (v2->x() - center->x()) / (v2->y() - center->y());
    }

    const qint32 div = (v2->x() - center->x()) * (v1->y() - center->y())
                     - (v2->y() - center->y()) * (v1->x() - center->x());
    const qint32 dd = div ? qint32((qint64(value * (v1->y() - v2->y())) << 8) / div) : 0;

    if (y2 < yC) {
        if (y1 < yC) {
            // Center at the bottom.
            if (y2 < y1) {
                // y2 < y1 < yC
                // Long right edge.
                d1 = centerFrac * value / (v1->y() - center->y());
                dd1 = ((value << 8) / (v1->y() - center->y()));
                fillLines<clip, BottomUp, LeftToRight>(bits, width, height, y1, yC, x1, dx1,
                                                       x2, dx2, d1, dd1, dd);
                dx1 = ((v1->x() - v2->x()) << 8) / (v1->y() - v2->y());
                x1 = v1->x() + v1Frac * (v1->x() - v2->x()) / (v1->y() - v2->y());
                fillLines<clip, BottomUp, LeftToRight>(bits, width, height, y2, y1, x1, dx1,
                                                       x2, dx2, value, 0, dd);
            } else {
                // y1 <= y2 < yC
                // Long left edge.
                d2 = centerFrac * value / (v2->y() - center->y());
                dd2 = ((value << 8) / (v2->y() - center->y()));
                fillLines<clip, BottomUp, RightToLeft>(bits, width, height, y2, yC, x1, dx1,
                                                       x2, dx2, d2, dd2, dd);
                if (y1 != y2) {
                    dx2 = ((v1->x() - v2->x()) << 8) / (v1->y() - v2->y());
                    x2 = v2->x() + v2Frac * (v1->x() - v2->x()) / (v1->y() - v2->y());
                    fillLines<clip, BottomUp, RightToLeft>(bits, width, height, y1, y2, x1, dx1,
                                                           x2, dx2, value, 0, dd);
                }
            }
        } else {
            // y2 < yC <= y1
            // Center to the right.
            int dx = ((v1->x() - v2->x()) << 8) / (v1->y() - v2->y());
            int xUp, xDn;
            xUp = xDn = v2->x() + (clip == Clip ? (yC << 8) + 0xff - v2->y()
                                                : (center->y() | 0xff) - v2->y())
                        * (v1->x() - v2->x()) / (v1->y() - v2->y());
            fillLines<clip, BottomUp, LeftToRight>(bits, width, height, y2, yC, xUp, dx,
                                                   x2, dx2, value, 0, dd);
            if (yC != y1)
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, yC, y1, xDn, dx,
                                                      x1, dx1, value, 0, dd);
        }
    } else {
        if (y1 < yC) {
            // y1 < yC <= y2
            // Center to the left.
            int dx = ((v1->x() - v2->x()) << 8) / (v1->y() - v2->y());
            int xUp, xDn;
            xUp = xDn = v1->x() + (clip == Clip ? (yC << 8) + 0xff - v1->y()
                                                : (center->y() | 0xff) - v1->y())
                        * (v1->x() - v2->x()) / (v1->y() - v2->y());
            fillLines<clip, BottomUp, RightToLeft>(bits, width, height, y1, yC, x1, dx1,
                                                   xUp, dx, value, 0, dd);
            if (yC != y2)
                fillLines<clip, TopDown, RightToLeft>(bits, width, height, yC, y2, x2, dx2,
                                                      xDn, dx, value, 0, dd);
        } else {
            // Center at the top.
            if (y2 < y1) {
                // yC <= y2 < y1
                // Long right edge.
                if (yC != y2) {
                    d2 = centerFrac * value / (v2->y() - center->y());
                    dd2 = ((value << 8) / (v2->y() - center->y()));
                    fillLines<clip, TopDown, LeftToRight>(bits, width, height, yC, y2, x2, dx2,
                                                          x1, dx1, d2, dd2, dd);
                }
                dx2 = ((v1->x() - v2->x()) << 8) / (v1->y() - v2->y());
                x2 = v2->x() + v2Frac * (v1->x() - v2->x()) / (v1->y() - v2->y());
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, y2, y1, x2, dx2,
                                                      x1, dx1, value, 0, dd);
            } else {
                // Long left edge.
                // yC <= y1 <= y2
                if (yC != y1) {
                    d1 = centerFrac * value / (v1->y() - center->y());
                    dd1 = ((value << 8) / (v1->y() - center->y()));
                    fillLines<clip, TopDown, RightToLeft>(bits, width, height, yC, y1, x2, dx2,
                                                          x1, dx1, d1, dd1, dd);
                }
                if (y1 != y2) {
                    dx1 = ((v1->x() - v2->x()) << 8) / (v1->y() - v2->y());
                    x1 = v1->x() + v1Frac * (v1->x() - v2->x()) / (v1->y() - v2->y());
                    fillLines<clip, TopDown, RightToLeft>(bits, width, height, y1, y2, x2, dx2,
                                                          x1, dx1, value, 0, dd);
                }
            }
        }
    }
}

template <FillClip clip>
void drawRectangle(qint32 *bits, int width, int height,
                   const QPoint *int1, const QPoint *center1, const QPoint *ext1,
                   const QPoint *int2, const QPoint *center2, const QPoint *ext2,
                   qint32 extValue)
{
    if (center1->y() > center2->y()) {
        qSwap(center1, center2);
        qSwap(int1, ext2);
        qSwap(ext1, int2);
        extValue = -extValue;
    }

    Q_ASSERT(ext1->x() - center1->x() == center1->x() - int1->x());
    Q_ASSERT(ext1->y() - center1->y() == center1->y() - int1->y());
    Q_ASSERT(ext2->x() - center2->x() == center2->x() - int2->x());
    Q_ASSERT(ext2->y() - center2->y() == center2->y() - int2->y());

    const int yc1 = clip == Clip ? qBound(0, center1->y() >> 8, height) : center1->y() >> 8;
    const int yc2 = clip == Clip ? qBound(0, center2->y() >> 8, height) : center2->y() >> 8;
    const int yi1 = clip == Clip ? qBound(0, int1->y() >> 8, height) : int1->y() >> 8;
    const int yi2 = clip == Clip ? qBound(0, int2->y() >> 8, height) : int2->y() >> 8;
    const int ye1 = clip == Clip ? qBound(0, ext1->y() >> 8, height) : ext1->y() >> 8;
    const int ye2 = clip == Clip ? qBound(0, ext2->y() >> 8, height) : ext2->y() >> 8;

    const int center1Frac = clip == Clip ? (yc1 << 8) + 0xff - center1->y() : ~center1->y() & 0xff;
    const int center2Frac = clip == Clip ? (yc2 << 8) + 0xff - center2->y() : ~center2->y() & 0xff;
    const int int1Frac = clip == Clip ? (yi1 << 8) + 0xff - int1->y() : ~int1->y() & 0xff;
    const int ext1Frac = clip == Clip ? (ye1 << 8) + 0xff - ext1->y() : ~ext1->y() & 0xff;

    int dxC = 0, dxE = 0; // cap slope, edge slope
    qint32 ddC = 0;
    if (ext1->y() != int1->y()) {
        dxC = ((ext1->x() - int1->x()) << 8) / (ext1->y() - int1->y());
        ddC = (extValue << 9) / (ext1->y() - int1->y());
    }
    if (ext1->y() != ext2->y())
        dxE = ((ext1->x() - ext2->x()) << 8) / (ext1->y() - ext2->y());

    const qint32 div = (ext1->x() - int1->x()) * (ext2->y() - int1->y())
                     - (ext1->y() - int1->y()) * (ext2->x() - int1->x());
    const qint32 dd = div ? qint32((qint64(extValue * (ext2->y() - ext1->y())) << 9) / div) : 0;

    int xe1, xe2, xc1, xc2;
    qint32 d;

    qint32 intValue = -extValue;

    if (center2->x() < center1->x()) {
        // Leaning to the right. '/'
        if (int1->y() < ext2->y()) {
            // Mostly vertical.
            Q_ASSERT(ext1->y() != ext2->y());
            xe1 = ext1->x() + ext1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
            xe2 = int1->x() + int1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
            if (ye1 != yi1) {
                xc2 = center1->x() + center1Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
                xc2 += (ye1 - yc1) * dxC;
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, ye1, yi1, xe1, dxE,
                                                      xc2, dxC, extValue, 0, dd);
            }
            if (yi1 != ye2)
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, yi1, ye2, xe1, dxE,
                                                      xe2, dxE, extValue, 0, dd);
            if (ye2 != yi2) {
                xc1 = center2->x() + center2Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
                xc1 += (ye2 - yc2) * dxC;
                fillLines<clip, TopDown, RightToLeft>(bits, width, height, ye2, yi2, xc1, dxC,
                                                      xe2, dxE, intValue, 0, dd);
            }
        } else {
            // Mostly horizontal.
            Q_ASSERT(ext1->y() != int1->y());
            xc1 = center2->x() + center2Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
            xc2 = center1->x() + center1Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
            xc1 += (ye2 - yc2) * dxC;
            xc2 += (ye1 - yc1) * dxC;
            if (ye1 != ye2) {
                xe1 = ext1->x() + ext1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, ye1, ye2, xe1, dxE,
                                                      xc2, dxC, extValue, 0, dd);
            }
            if (ye2 != yi1) {
                d = (clip == Clip ? (ye2 << 8) + 0xff - center2->y()
                                  : (ext2->y() | 0xff) - center2->y())
                    * 2 * extValue / (ext1->y() - int1->y());
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, ye2, yi1, xc1, dxC,
                                                      xc2, dxC, d, ddC, dd);
            }
            if (yi1 != yi2) {
                xe2 = int1->x() + int1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
                fillLines<clip, TopDown, RightToLeft>(bits, width, height, yi1, yi2, xc1, dxC,
                                                      xe2, dxE, intValue, 0, dd);
            }
        }
    } else {
        // Leaning to the left. '\'
        if (ext1->y() < int2->y()) {
            // Mostly vertical.
            Q_ASSERT(ext1->y() != ext2->y());
            xe1 = ext1->x() + ext1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
            xe2 = int1->x() + int1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
            if (yi1 != ye1) {
                xc1 = center1->x() + center1Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
                xc1 += (yi1 - yc1) * dxC;
                fillLines<clip, TopDown, RightToLeft>(bits, width, height, yi1, ye1, xc1, dxC,
                                                      xe2, dxE, intValue, 0, dd);
            }
            if (ye1 != yi2)
                fillLines<clip, TopDown, RightToLeft>(bits, width, height, ye1, yi2, xe1, dxE,
                                                      xe2, dxE, intValue, 0, dd);
            if (yi2 != ye2) {
                xc2 = center2->x() + center2Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
                xc2 += (yi2 - yc2) * dxC;
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, yi2, ye2, xe1, dxE,
                                                      xc2, dxC, extValue, 0, dd);
            }
        } else {
            // Mostly horizontal.
            Q_ASSERT(ext1->y() != int1->y());
            xc1 = center1->x() + center1Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
            xc2 = center2->x() + center2Frac * (ext1->x() - int1->x()) / (ext1->y() - int1->y());
            xc1 += (yi1 - yc1) * dxC;
            xc2 += (yi2 - yc2) * dxC;
            if (yi1 != yi2) {
                xe2 = int1->x() + int1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
                fillLines<clip, TopDown, RightToLeft>(bits, width, height, yi1, yi2, xc1, dxC,
                                                      xe2, dxE, intValue, 0, dd);
            }
            if (yi2 != ye1) {
                d = (clip == Clip ? (yi2 << 8) + 0xff - center2->y()
                                  : (int2->y() | 0xff) - center2->y())
                    * 2 * extValue / (ext1->y() - int1->y());
                fillLines<clip, TopDown, RightToLeft>(bits, width, height, yi2, ye1, xc1, dxC,
                                                      xc2, dxC, d, ddC, dd);
            }
            if (ye1 != ye2) {
                xe1 = ext1->x() + ext1Frac * (ext1->x() - ext2->x()) / (ext1->y() - ext2->y());
                fillLines<clip, TopDown, LeftToRight>(bits, width, height, ye1, ye2, xe1, dxE,
                                                      xc2, dxC, extValue, 0, dd);
            }
        }
    }
}

static void drawPolygons(qint32 *bits, int width, int height, const QPoint *vertices,
                         const quint32 *indices, int indexCount, qint32 value)
{
    Q_ASSERT(indexCount != 0);
    typedef QVarLengthArray<quint16, 16> ScanLine;
    QVarLengthArray<ScanLine, 128> scans(height);
    int first = 0;
    for (int i = 1; i < indexCount; ++i) {
        quint32 idx1 = indices[i - 1];
        quint32 idx2 = indices[i];
        Q_ASSERT(idx1 != quint32(-1));
        if (idx2 == quint32(-1)) {
            idx2 = indices[first];
            Q_ASSERT(idx2 != quint32(-1));
            first = ++i;
        }
        const QPoint *v1 = &vertices[idx1];
        const QPoint *v2 = &vertices[idx2];
        if (v2->y() < v1->y())
            qSwap(v1, v2);
        int fromY = qMax(0, v1->y() >> 8);
        int toY = qMin(height, v2->y() >> 8);
        if (fromY >= toY)
            continue;
        int dx = ((v2->x() - v1->x()) << 8) / (v2->y() - v1->y());
        int x = v1->x() + ((fromY << 8) + 0xff - v1->y()) * (v2->x() - v1->x()) / (v2->y() - v1->y());
        for (int y = fromY; y < toY; ++y) {
            quint32 c = quint32(x >> 8);
            if (c < quint32(width))
                scans[y].append(quint16(c));
            x += dx;
        }
    }
    for (int i = 0; i < height; ++i) {
        quint16 *scanline = scans[i].data();
        int size = scans[i].size();
        for (int j = 1; j < size; ++j) {
            int k = j;
            quint16 value = scanline[k];
            for (; k != 0 && value < scanline[k - 1]; --k)
                scanline[k] = scanline[k - 1];
            scanline[k] = value;
        }
        qint32 *line = bits + i * width;
        int j = 0;
        for (; j + 1 < size; j += 2) {
            for (quint16 x = scanline[j]; x < scanline[j + 1]; ++x)
                line[x] = value;
        }
        if (j < size) {
            for (int x = scanline[j]; x < width; ++x)
                line[x] = value;
        }
    }
}

static void makeDistanceField(unsigned char *output, int imgWidth, int imgHeight,
    const QVarLengthArray<QPoint> &pathVertices, const QVarLengthArray<quint32> &pathIndices, int offs)
{
    const qint32 interiorColor = -0x7f80; // 8:8 signed format, -127.5
    const qint32 exteriorColor = 0x7f80; // 8:8 signed format, 127.5

    QScopedArrayPointer<qint32> bits(new qint32[imgWidth * imgHeight]);
    for (int i = 0; i < imgWidth * imgHeight; ++i)
        bits[i] = exteriorColor;

    const qreal angleStep = qDegreesToRadians(qreal(15));
    const QPoint rotation(qRound(qCos(angleStep) * 0x4000),
                          qRound(qSin(angleStep) * 0x4000)); // 2:14 signed

    const quint32 *indices = pathIndices.data();
    QVarLengthArray<QPoint> normals;
    QVarLengthArray<QPoint> vertices;

    QVarLengthArray<int> crossProducts;
    QVarLengthArray<bool> needsClipping;

    drawPolygons(bits.data(), imgWidth, imgHeight, pathVertices.data(),
                 indices, pathIndices.size(), interiorColor);

    int index = 0;

    while (index < pathIndices.size()) {
        normals.clear();
        vertices.clear();
        needsClipping.clear();

        // Find end of polygon.
        int end = index;
        while (indices[end] != quint32(-1))
            ++end;

        // Calculate vertex normals.
        for (int next = index, prev = end - 1; next < end; prev = next++) {
            quint32 fromVertexIndex = indices[prev];
            quint32 toVertexIndex = indices[next];

            const QPoint &from = pathVertices.at(fromVertexIndex);
            const QPoint &to = pathVertices.at(toVertexIndex);

            QPoint n(to.y() - from.y(), from.x() - to.x());
            if (n.x() == 0 && n.y() == 0)
                continue;
            int scale = qRound((offs << 16) / qSqrt(qreal(n.x()) * n.x() + qreal(n.y()) * n.y())); // 8:16
            Q_ASSERT(scale != 0);

            n.rx() = n.x() * scale >> 8;
            n.ry() = n.y() * scale >> 8;
            normals.append(n);
            QPoint v(to.x() + 0x7f, to.y() + 0x7f);
            vertices.append(v);
            needsClipping.append((to.x() < offs << 8) || (to.x() >= (imgWidth - offs) << 8)
                                 || (to.y() < offs << 8) || (to.y() >= (imgHeight - offs) << 8));
        }

        crossProducts.resize(normals.size());
        for (int next = 0, prev = normals.size() - 1; next < normals.size(); prev = next++) {
            crossProducts[prev] = normals.at(prev).x() * normals.at(next).y()
                                - normals.at(prev).y() * normals.at(next).x();
        }

        // Draw quads.
        for (int next = 0, prev = normals.size() - 1; next < normals.size(); prev = next++) {
            QPoint n = normals.at(next);
            QPoint intPrev = vertices.at(prev);
            QPoint extPrev = vertices.at(prev);
            QPoint intNext = vertices.at(next);
            QPoint extNext = vertices.at(next);

            extPrev.rx() -= n.x();
            extPrev.ry() -= n.y();
            intPrev.rx() += n.x();
            intPrev.ry() += n.y();
            extNext.rx() -= n.x();
            extNext.ry() -= n.y();
            intNext.rx() += n.x();
            intNext.ry() += n.y();

            if (needsClipping[prev] || needsClipping[next]) {
                drawRectangle<Clip>(bits.data(), imgWidth, imgHeight,
                                    &intPrev, &vertices.at(prev), &extPrev,
                                    &intNext, &vertices.at(next), &extNext,
                                    exteriorColor);
            } else {
                drawRectangle<NoClip>(bits.data(), imgWidth, imgHeight,
                                      &intPrev, &vertices.at(prev), &extPrev,
                                      &intNext, &vertices.at(next), &extNext,
                                      exteriorColor);
            }

            const int crossProduct = crossProducts.at(prev);
            if (crossProduct < 0) {
                QPoint p = extPrev;
                if (needsClipping[prev]) {
                    for (;;) {
                        QPoint rn((n.x() * rotation.x() - n.y() * rotation.y()) >> 14,
                                  (n.y() * rotation.x() + n.x() * rotation.y()) >> 14);
                        n = rn;
                        if (n.x() * normals.at(prev).y() - n.y() * normals.at(prev).x() <= 0) {
                            p.rx() = vertices.at(prev).x() - normals.at(prev).x();
                            p.ry() = vertices.at(prev).y() - normals.at(prev).y();
                            drawTriangle<Clip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                               &extPrev, &p, exteriorColor);
                            break;
                        }

                        p.rx() = vertices.at(prev).x() - n.x();
                        p.ry() = vertices.at(prev).y() - n.y();
                        drawTriangle<Clip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                           &extPrev, &p, exteriorColor);
                        extPrev = p;
                    }
                } else {
                    for (;;) {
                        QPoint rn((n.x() * rotation.x() - n.y() * rotation.y()) >> 14,
                                  (n.y() * rotation.x() + n.x() * rotation.y()) >> 14);
                        n = rn;
                        if (n.x() * normals.at(prev).y() - n.y() * normals.at(prev).x() <= 0) {
                            p.rx() = vertices.at(prev).x() - normals.at(prev).x();
                            p.ry() = vertices.at(prev).y() - normals.at(prev).y();
                            drawTriangle<NoClip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                                 &extPrev, &p, exteriorColor);
                            break;
                        }

                        p.rx() = vertices.at(prev).x() - n.x();
                        p.ry() = vertices.at(prev).y() - n.y();
                        drawTriangle<NoClip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                             &extPrev, &p, exteriorColor);
                        extPrev = p;
                    }
                }
            } else if (crossProduct > 0) {
                QPoint p = intPrev;
                if (needsClipping[prev]) {
                    for (;;) {
                        QPoint rn((n.x() * rotation.x() + n.y() * rotation.y()) >> 14,
                                  (n.y() * rotation.x() - n.x() * rotation.y()) >> 14);
                        n = rn;
                        if (n.x() * normals.at(prev).y() - n.y() * normals.at(prev).x() >= 0) {
                            p.rx() = vertices.at(prev).x() + normals.at(prev).x();
                            p.ry() = vertices.at(prev).y() + normals.at(prev).y();
                            drawTriangle<Clip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                               &p, &intPrev, interiorColor);
                            break;
                        }

                        p.rx() = vertices.at(prev).x() + n.x();
                        p.ry() = vertices.at(prev).y() + n.y();
                        drawTriangle<Clip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                           &p, &intPrev, interiorColor);
                        intPrev = p;
                    }
                } else {
                    for (;;) {
                        QPoint rn((n.x() * rotation.x() + n.y() * rotation.y()) >> 14,
                                  (n.y() * rotation.x() - n.x() * rotation.y()) >> 14);
                        n = rn;
                        if (n.x() * normals.at(prev).y() - n.y() * normals.at(prev).x() >= 0) {
                            p.rx() = vertices.at(prev).x() + normals.at(prev).x();
                            p.ry() = vertices.at(prev).y() + normals.at(prev).y();
                            drawTriangle<NoClip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                                 &p, &intPrev, interiorColor);
                            break;
                        }

                        p.rx() = vertices.at(prev).x() + n.x();
                        p.ry() = vertices.at(prev).y() + n.y();
                        drawTriangle<NoClip>(bits.data(), imgWidth, imgHeight, &vertices.at(prev),
                                             &p, &intPrev, interiorColor);
                        intPrev = p;
                    }
                }
            }
        }

        index = end + 1;
    }

    const qint32 *inLine = bits.data();
    uchar *outLine = output;
    for (int y = 0; y < imgHeight; ++y) {
        for (int x = 0; x < imgWidth; ++x, ++inLine, ++outLine)
            *outLine = uchar((0x7f80 - *inLine) >> 8);
    }
}

extern "C" void hyprv_distance_field(const int32_t *xy, size_t count, const uint32_t *idx, size_t nidx, uint8_t *output, int width, int height, int radius) {
    QVarLengthArray<QPoint> vertices; for(size_t i=0;i<count;i++)vertices.emplace_back(xy[2*i],xy[2*i+1]);
    QVarLengthArray<quint32> indices(idx,idx+nidx);
    makeDistanceField(output,width,height,vertices,indices,radius);
}
