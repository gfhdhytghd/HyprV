// Copyright (C) 2016 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Standalone Qt 6.12 QRasterizer::rasterizeLine / intersectPixelFP.
// Geometry/storage adapters use only the standard library. No Qt linkage.
#include <algorithm>
#include <cassert>
#include <climits>
#include <cmath>
#include <cstdint>
using qreal = double;
using QScFixed = int64_t;
#define Q_PROCESSOR_WORDSIZE 8
#define Q_ASSERT(x) assert(x)
#define IntToQScFixed(i) ((QScFixed)(i) * 65536)
#define QScFixedToInt(i) int((i) >> 16)
#define QScFixedFactor 65536
#define QScFixedMultiply(a, b) (((a) * (b)) >> 16)
#define QScFixedFastMultiply(a, b) (((a) * (b)) >> 16)
#define qAbs std::abs
#define qSqrt std::sqrt
#define qMin std::min
#define qMax std::max
#define qSwap std::swap
template <class T> T qBound(T a, T b, T c) { return std::clamp(b, a, c); }
struct QPointF {
  double xx = 0, yy = 0;
  QPointF() = default;
  QPointF(double x, double y) : xx(x), yy(y) {}
  double x() const { return xx; }
  double y() const { return yy; }
  double &rx() { return xx; }
  double &ry() { return yy; }
  QPointF operator+(QPointF b) const { return {xx + b.xx, yy + b.yy}; }
  QPointF operator-(QPointF b) const { return {xx - b.xx, yy - b.yy}; }
  QPointF operator*(double k) const { return {xx * k, yy * k}; }
  QPointF &operator+=(QPointF b) {
    xx += b.xx;
    yy += b.yy;
    return *this;
  }
  QPointF &operator-=(QPointF b) {
    xx -= b.xx;
    yy -= b.yy;
    return *this;
  }
  QPointF &operator*=(double k) {
    xx *= k;
    yy *= k;
    return *this;
  }
  bool operator==(QPointF b) const { return xx == b.xx && yy == b.yy; }
};
inline QPointF operator*(double k, QPointF p) { return p * k; }
using QPoint = QPointF;
struct QRectF {
  QPointF a, b;
  QRectF(QPointF x, QPointF y) : a(x), b(y) {}
};
struct ClipRect {
  int w, h;
  bool isEmpty() const { return w <= 0 || h <= 0; }
  int left() const { return 0; }
  int top() const { return 0; }
  int right() const { return w - 1; }
  int bottom() const { return h - 1; }
  QPointF topLeft() const { return {0, 0}; }
  QPointF bottomRight() const { return {double(w - 1), double(h - 1)}; }
};
bool qClipLine(QPointF *a, QPointF *b, const QRectF &r) {
  double t0 = 0, t1 = 1;
  auto delta = *b - *a;
  auto clip = [&](double p, double q) {
    if (p == 0)
      return q >= 0;
    double t = q / p;
    if (p < 0) {
      if (t > t1)
        return false;
      t0 = std::max(t0, t);
    } else {
      if (t < t0)
        return false;
      t1 = std::min(t1, t);
    }
    return true;
  };
  if (!clip(-delta.x(), a->x() - r.a.x()) ||
      !clip(delta.x(), r.b.x() - a->x()) ||
      !clip(-delta.y(), a->y() - r.a.y()) || !clip(delta.y(), r.b.y() - a->y()))
    return true;
  auto start = *a;
  *a = start + delta * t0;
  *b = start + delta * t1;
  return false;
}
using SpanCallback = void (*)(void *, int, int, int, unsigned);
struct D {
  ClipRect clipRect;
  SpanCallback blend;
  void *data;
  bool antialiased = true;
};
struct QSpanBuffer {
  SpanCallback callback;
  void *cookie;
  ClipRect clip;
  QSpanBuffer(SpanCallback a, void *b, ClipRect c)
      : callback(a), cookie(b), clip(c) {}
  void addSpan(int x, int len, int y, unsigned coverage) {
    int left = std::max(x, 0), right = std::min(x + len, clip.w);
    if (y >= 0 && y < clip.h && right > left && coverage)
      callback(cookie, left, y, right - left, std::min(coverage, 255u));
  }
};
static QScFixed intersectPixelFP(int x, QScFixed top, QScFixed bottom,
                                 QScFixed leftIntersectX,
                                 QScFixed rightIntersectX, QScFixed slope,
                                 QScFixed invSlope) {
  QScFixed leftX = IntToQScFixed(x);
  QScFixed rightX = IntToQScFixed(x) + QScFixedFactor;

  QScFixed leftIntersectY, rightIntersectY;
  auto computeIntersectY = [&]() {
    if (slope > 0) {
      leftIntersectY = top + QScFixedMultiply(leftX - leftIntersectX, invSlope);
      rightIntersectY = leftIntersectY + invSlope;
    } else {
      leftIntersectY =
          top + QScFixedMultiply(leftX - rightIntersectX, invSlope);
      rightIntersectY = leftIntersectY + invSlope;
    }
  };

  if (leftIntersectX >= leftX && rightIntersectX <= rightX) {
    return QScFixedMultiply(bottom - top,
                            leftIntersectX - leftX +
                                ((rightIntersectX - leftIntersectX) >> 1));
  } else if (leftIntersectX >= rightX) {
    return bottom - top;
  } else if (leftIntersectX >= leftX) {
    computeIntersectY();
    if (slope > 0) {
      return (bottom - top) -
             QScFixedFastMultiply((rightX - leftIntersectX) >> 1,
                                  rightIntersectY - top);
    } else {
      return (bottom - top) -
             QScFixedFastMultiply((rightX - leftIntersectX) >> 1,
                                  bottom - rightIntersectY);
    }
  } else if (rightIntersectX <= leftX) {
    return 0;
  } else if (rightIntersectX <= rightX) {
    computeIntersectY();
    if (slope > 0) {
      return QScFixedFastMultiply((rightIntersectX - leftX) >> 1,
                                  bottom - leftIntersectY);
    } else {
      return QScFixedFastMultiply((rightIntersectX - leftX) >> 1,
                                  leftIntersectY - top);
    }
  } else {
    computeIntersectY();
    if (slope > 0) {
      return (bottom - rightIntersectY) +
             ((rightIntersectY - leftIntersectY) >> 1);
    } else {
      return (rightIntersectY - top) +
             ((leftIntersectY - rightIntersectY) >> 1);
    }
  }
}

static inline bool q26Dot6Compare(qreal p1, qreal p2) {
  return int((p2 - p1) * 64.) == 0;
}

static inline QPointF snapTo26Dot6Grid(const QPointF &p) {
  return QPointF(std::floor(p.x() * 64) * (1 / qreal(64)),
                 std::floor(p.y() * 64) * (1 / qreal(64)));
}

/*
   The rasterize line function relies on some div by zero which should
   result in +/-inf values. However, when floating point exceptions are
   enabled, this will cause crashes, so we return high numbers instead.
   As the returned value is used in further arithmetic, returning
   FLT_MAX/DBL_MAX will also cause values, so instead return a value
   that is well outside the int-range.
 */
static inline qreal qSafeDivide(qreal x, qreal y) {
  if (y == 0)
    return x > 0 ? 1e20 : -1e20;
  return x / y;
}

/* Conversion to int fails if the value is too large to fit into INT_MAX or
   too small to fit into INT_MIN, so we need this slightly safer conversion
   when floating point exceptions are enabled
 */
static inline QScFixed qSafeFloatToQScFixed(qreal x) {
  qreal tmp = x * QScFixedFactor;
#if Q_PROCESSOR_WORDSIZE == 8
  if (tmp > qreal(INT_MAX) * QScFixedFactor)
    return QScFixed(INT_MAX) * QScFixedFactor;
  else if (tmp < qreal(INT_MIN) * QScFixedFactor)
    return QScFixed(INT_MIN) * QScFixedFactor;
#else
  if (tmp > qreal(INT_MAX))
    return INT_MAX;
  else if (tmp < qreal(INT_MIN))
    return -INT_MAX;
#endif
  return QScFixed(tmp);
}
extern "C" void hyprv_qt_raster_line(double ax, double ay, double bx, double by,
                                     double width, bool squareCap, int w, int h,
                                     SpanCallback callback, void *cookie) {
  QPointF a(ax, ay), b(bx, by);
  D state{ClipRect{w, h}, callback, cookie};
  D *d = &state;
  if (a == b || !(width > 0.0) || d->clipRect.isEmpty())
    return;

  QPointF pa = a;
  QPointF pb = b;

  if (squareCap) {
    QPointF delta = pb - pa;
    pa -= (0.5f * width) * delta;
    pb += (0.5f * width) * delta;
  }

  QPointF offs =
      QPointF(qAbs(b.y() - a.y()), qAbs(b.x() - a.x())) * width * 0.5;
  const QRectF clip(d->clipRect.topLeft() - offs,
                    d->clipRect.bottomRight() + QPoint(1, 1) + offs);
  if (qClipLine(&pa, &pb, clip))
    return;

  {
    // old delta
    const QPointF d0 = a - b;
    const qreal w0 = d0.x() * d0.x() + d0.y() * d0.y();

    // new delta
    const QPointF d = pa - pb;
    const qreal w = d.x() * d.x() + d.y() * d.y();

    if (w == 0)
      return;

    // adjust width which is given relative to |b - a|
    width *= qSqrt(w0 / w);
  }

  QSpanBuffer buffer(d->blend, d->data, d->clipRect);

  if (q26Dot6Compare(pa.y(), pb.y())) {
    const qreal x = (pa.x() + pb.x()) * 0.5f;
    const qreal dx = qAbs(pb.x() - pa.x()) * 0.5f;

    const qreal y = pa.y();
    const qreal dy = width * dx;

    pa = QPointF(x, y - dy);
    pb = QPointF(x, y + dy);

    width = 1 / width;
  }

  if (q26Dot6Compare(pa.x(), pb.x())) {
    if (pa.y() > pb.y())
      qSwap(pa, pb);

    const qreal dy = pb.y() - pa.y();
    const qreal halfWidth = 0.5f * width * dy;

    qreal left = pa.x() - halfWidth;
    qreal right = pa.x() + halfWidth;

    left =
        qBound(qreal(d->clipRect.left()), left, qreal(d->clipRect.right() + 1));
    right = qBound(qreal(d->clipRect.left()), right,
                   qreal(d->clipRect.right() + 1));

    pa.ry() = qBound(qreal(d->clipRect.top()), pa.y(),
                     qreal(d->clipRect.bottom() + 1));
    pb.ry() = qBound(qreal(d->clipRect.top()), pb.y(),
                     qreal(d->clipRect.bottom() + 1));

    if (q26Dot6Compare(left, right) || q26Dot6Compare(pa.y(), pb.y()))
      return;

    if (d->antialiased) {
      const int iLeft = int(left);
      const int iRight = int(right);
      const QScFixed leftWidth =
          IntToQScFixed(iLeft + 1) - qSafeFloatToQScFixed(left);
      const QScFixed rightWidth =
          qSafeFloatToQScFixed(right) - IntToQScFixed(iRight);

      QScFixed coverage[3];
      int x[3];
      int len[3];

      int n = 1;
      if (iLeft == iRight) {
        if (leftWidth == QScFixedFactor)
          coverage[0] = rightWidth * 255;
        else
          coverage[0] = (rightWidth + leftWidth - QScFixedFactor) * 255;
        x[0] = iLeft;
        len[0] = 1;
      } else {
        coverage[0] = leftWidth * 255;
        x[0] = iLeft;
        len[0] = 1;
        if (leftWidth == QScFixedFactor) {
          len[0] = iRight - iLeft;
        } else if (iRight - iLeft > 1) {
          coverage[1] = IntToQScFixed(255);
          x[1] = iLeft + 1;
          len[1] = iRight - iLeft - 1;
          ++n;
        }
        if (rightWidth) {
          coverage[n] = rightWidth * 255;
          x[n] = iRight;
          len[n] = 1;
          ++n;
        }
      }

      const QScFixed iTopFP = IntToQScFixed(int(pa.y()));
      const QScFixed iBottomFP = IntToQScFixed(int(pb.y()));
      const QScFixed yPa = qSafeFloatToQScFixed(pa.y());
      const QScFixed yPb = qSafeFloatToQScFixed(pb.y());
      for (QScFixed yFP = iTopFP; yFP <= iBottomFP; yFP += QScFixedFactor) {
        const QScFixed rowHeight =
            qMin(yFP + QScFixedFactor, yPb) - qMax(yFP, yPa);
        const int y = QScFixedToInt(yFP);
        if (y > d->clipRect.bottom())
          break;
        for (int i = 0; i < n; ++i) {
          buffer.addSpan(
              x[i], len[i], y,
              QScFixedToInt(QScFixedMultiply(rowHeight, coverage[i])));
        }
      }
    } else { // aliased
      int iTop = int(pa.y() + 0.5f);
      int iBottom = pb.y() < 0.5f ? -1 : int(pb.y() - 0.5f);
      int iLeft = int(left + 0.5f);
      int iRight = right < 0.5f ? -1 : int(right - 0.5f);

      int iWidth = iRight - iLeft + 1;
      for (int y = iTop; y <= iBottom; ++y)
        buffer.addSpan(iLeft, iWidth, y, 255);
    }
  } else {
    if (pa.y() > pb.y())
      qSwap(pa, pb);

    QPointF delta = pb - pa;
    delta *= 0.5f * width;
    const QPointF perp(delta.y(), -delta.x());

    QPointF top;
    QPointF left;
    QPointF right;
    QPointF bottom;

    if (pa.x() < pb.x()) {
      top = pa + perp;
      left = pa - perp;
      right = pb + perp;
      bottom = pb - perp;
    } else {
      top = pa - perp;
      left = pb - perp;
      right = pa + perp;
      bottom = pb + perp;
    }

    top = snapTo26Dot6Grid(top);
    bottom = snapTo26Dot6Grid(bottom);
    left = snapTo26Dot6Grid(left);
    right = snapTo26Dot6Grid(right);

    const qreal topBound =
        qBound(qreal(d->clipRect.top()), top.y(), qreal(d->clipRect.bottom()));
    const qreal bottomBound = qBound(qreal(d->clipRect.top()), bottom.y(),
                                     qreal(d->clipRect.bottom()));

    const QPointF topLeftEdge = left - top;
    const QPointF topRightEdge = right - top;
    const QPointF bottomLeftEdge = bottom - left;
    const QPointF bottomRightEdge = bottom - right;

    const qreal topLeftSlope = qSafeDivide(topLeftEdge.x(), topLeftEdge.y());
    const qreal bottomLeftSlope =
        qSafeDivide(bottomLeftEdge.x(), bottomLeftEdge.y());

    const qreal topRightSlope = qSafeDivide(topRightEdge.x(), topRightEdge.y());
    const qreal bottomRightSlope =
        qSafeDivide(bottomRightEdge.x(), bottomRightEdge.y());

    const QScFixed topLeftSlopeFP = qSafeFloatToQScFixed(topLeftSlope);
    const QScFixed topRightSlopeFP = qSafeFloatToQScFixed(topRightSlope);

    const QScFixed bottomLeftSlopeFP = qSafeFloatToQScFixed(bottomLeftSlope);
    const QScFixed bottomRightSlopeFP = qSafeFloatToQScFixed(bottomRightSlope);

    const QScFixed invTopLeftSlopeFP =
        qSafeFloatToQScFixed(qSafeDivide(1, topLeftSlope));
    const QScFixed invTopRightSlopeFP =
        qSafeFloatToQScFixed(qSafeDivide(1, topRightSlope));

    const QScFixed invBottomLeftSlopeFP =
        qSafeFloatToQScFixed(qSafeDivide(1, bottomLeftSlope));
    const QScFixed invBottomRightSlopeFP =
        qSafeFloatToQScFixed(qSafeDivide(1, bottomRightSlope));

    if (d->antialiased) {
      const QScFixed iTopFP = IntToQScFixed(int(topBound));
      const QScFixed iLeftFP = IntToQScFixed(int(left.y()));
      const QScFixed iRightFP = IntToQScFixed(int(right.y()));
      const QScFixed iBottomFP = IntToQScFixed(int(bottomBound));

      QScFixed leftIntersectAf = qSafeFloatToQScFixed(
          top.x() + (int(topBound) - top.y()) * topLeftSlope);
      QScFixed rightIntersectAf = qSafeFloatToQScFixed(
          top.x() + (int(topBound) - top.y()) * topRightSlope);
      QScFixed leftIntersectBf = 0;
      QScFixed rightIntersectBf = 0;

      if (iLeftFP < iTopFP)
        leftIntersectBf = qSafeFloatToQScFixed(
            left.x() + (int(topBound) - left.y()) * bottomLeftSlope);

      if (iRightFP < iTopFP)
        rightIntersectBf = qSafeFloatToQScFixed(
            right.x() + (int(topBound) - right.y()) * bottomRightSlope);

      QScFixed rowTop, rowBottomLeft, rowBottomRight, rowTopLeft, rowTopRight,
          rowBottom;
      QScFixed topLeftIntersectAf, topLeftIntersectBf, topRightIntersectAf,
          topRightIntersectBf;
      QScFixed bottomLeftIntersectAf, bottomLeftIntersectBf,
          bottomRightIntersectAf, bottomRightIntersectBf;

      int leftMin, leftMax, rightMin, rightMax;

      const QScFixed yTopFP = qSafeFloatToQScFixed(top.y());
      const QScFixed yLeftFP = qSafeFloatToQScFixed(left.y());
      const QScFixed yRightFP = qSafeFloatToQScFixed(right.y());
      const QScFixed yBottomFP = qSafeFloatToQScFixed(bottom.y());

      rowTop = qMax(iTopFP, yTopFP);
      topLeftIntersectAf =
          leftIntersectAf + QScFixedMultiply(topLeftSlopeFP, rowTop - iTopFP);
      topRightIntersectAf =
          rightIntersectAf + QScFixedMultiply(topRightSlopeFP, rowTop - iTopFP);

      QScFixed yFP = iTopFP;
      while (yFP <= iBottomFP) {
        rowBottomLeft = qMin(yFP + QScFixedFactor, yLeftFP);
        rowBottomRight = qMin(yFP + QScFixedFactor, yRightFP);
        rowTopLeft = qMax(yFP, yLeftFP);
        rowTopRight = qMax(yFP, yRightFP);
        rowBottom = qMin(yFP + QScFixedFactor, yBottomFP);

        if (yFP == iLeftFP) {
          const int y = QScFixedToInt(yFP);
          leftIntersectBf =
              qSafeFloatToQScFixed(left.x() + (y - left.y()) * bottomLeftSlope);
          topLeftIntersectBf =
              leftIntersectBf +
              QScFixedMultiply(bottomLeftSlopeFP, rowTopLeft - yFP);
          bottomLeftIntersectAf =
              leftIntersectAf +
              QScFixedMultiply(topLeftSlopeFP, rowBottomLeft - yFP);
        } else {
          topLeftIntersectBf = leftIntersectBf;
          bottomLeftIntersectAf = leftIntersectAf + topLeftSlopeFP;
        }

        if (yFP == iRightFP) {
          const int y = QScFixedToInt(yFP);
          rightIntersectBf = qSafeFloatToQScFixed(
              right.x() + (y - right.y()) * bottomRightSlope);
          topRightIntersectBf =
              rightIntersectBf +
              QScFixedMultiply(bottomRightSlopeFP, rowTopRight - yFP);
          bottomRightIntersectAf =
              rightIntersectAf +
              QScFixedMultiply(topRightSlopeFP, rowBottomRight - yFP);
        } else {
          topRightIntersectBf = rightIntersectBf;
          bottomRightIntersectAf = rightIntersectAf + topRightSlopeFP;
        }

        if (yFP == iBottomFP) {
          bottomLeftIntersectBf =
              leftIntersectBf +
              QScFixedMultiply(bottomLeftSlopeFP, rowBottom - yFP);
          bottomRightIntersectBf =
              rightIntersectBf +
              QScFixedMultiply(bottomRightSlopeFP, rowBottom - yFP);
        } else {
          bottomLeftIntersectBf = leftIntersectBf + bottomLeftSlopeFP;
          bottomRightIntersectBf = rightIntersectBf + bottomRightSlopeFP;
        }

        if (yFP < iLeftFP) {
          leftMin = QScFixedToInt(bottomLeftIntersectAf);
          leftMax = QScFixedToInt(topLeftIntersectAf);
        } else if (yFP == iLeftFP) {
          leftMin =
              QScFixedToInt(qMax(bottomLeftIntersectAf, topLeftIntersectBf));
          leftMax =
              QScFixedToInt(qMax(topLeftIntersectAf, bottomLeftIntersectBf));
        } else {
          leftMin = QScFixedToInt(topLeftIntersectBf);
          leftMax = QScFixedToInt(bottomLeftIntersectBf);
        }

        leftMin = qBound(d->clipRect.left(), leftMin, d->clipRect.right());
        leftMax = qBound(d->clipRect.left(), leftMax, d->clipRect.right());

        if (yFP < iRightFP) {
          rightMin = QScFixedToInt(topRightIntersectAf);
          rightMax = QScFixedToInt(bottomRightIntersectAf);
        } else if (yFP == iRightFP) {
          rightMin =
              QScFixedToInt(qMin(topRightIntersectAf, bottomRightIntersectBf));
          rightMax =
              QScFixedToInt(qMin(bottomRightIntersectAf, topRightIntersectBf));
        } else {
          rightMin = QScFixedToInt(bottomRightIntersectBf);
          rightMax = QScFixedToInt(topRightIntersectBf);
        }

        rightMin = qBound(d->clipRect.left(), rightMin, d->clipRect.right());
        rightMax = qBound(d->clipRect.left(), rightMax, d->clipRect.right());

        if (leftMax > rightMax)
          leftMax = rightMax;
        if (rightMin < leftMin)
          rightMin = leftMin;

        QScFixed rowHeight = rowBottom - rowTop;

        int x = leftMin;
        while (x <= leftMax) {
          QScFixed excluded = 0;

          if (yFP <= iLeftFP && rowBottomLeft > rowTop)
            excluded += intersectPixelFP(
                x, rowTop, rowBottomLeft, bottomLeftIntersectAf,
                topLeftIntersectAf, topLeftSlopeFP, invTopLeftSlopeFP);
          if (yFP >= iLeftFP && rowBottom > rowTopLeft)
            excluded += intersectPixelFP(
                x, rowTopLeft, rowBottom, topLeftIntersectBf,
                bottomLeftIntersectBf, bottomLeftSlopeFP, invBottomLeftSlopeFP);
          if (x >= rightMin) {
            if (yFP <= iRightFP && rowBottomRight > rowTop)
              excluded +=
                  (rowBottomRight - rowTop) -
                  intersectPixelFP(x, rowTop, rowBottomRight,
                                   topRightIntersectAf, bottomRightIntersectAf,
                                   topRightSlopeFP, invTopRightSlopeFP);
            if (yFP >= iRightFP && rowBottom > rowTopRight)
              excluded +=
                  (rowBottom - rowTopRight) -
                  intersectPixelFP(x, rowTopRight, rowBottom,
                                   bottomRightIntersectBf, topRightIntersectBf,
                                   bottomRightSlopeFP, invBottomRightSlopeFP);
          }

          Q_ASSERT(excluded >= 0 && excluded <= rowHeight);
          QScFixed coverage = rowHeight - excluded;
          buffer.addSpan(x, 1, QScFixedToInt(yFP),
                         QScFixedToInt(255 * coverage));
          ++x;
        }
        if (x < rightMin) {
          buffer.addSpan(x, rightMin - x, QScFixedToInt(yFP),
                         QScFixedToInt(255 * rowHeight));
          x = rightMin;
        }
        while (x <= rightMax) {
          QScFixed excluded = 0;
          if (yFP <= iRightFP && rowBottomRight > rowTop)
            excluded +=
                (rowBottomRight - rowTop) -
                intersectPixelFP(x, rowTop, rowBottomRight, topRightIntersectAf,
                                 bottomRightIntersectAf, topRightSlopeFP,
                                 invTopRightSlopeFP);
          if (yFP >= iRightFP && rowBottom > rowTopRight)
            excluded +=
                (rowBottom - rowTopRight) -
                intersectPixelFP(x, rowTopRight, rowBottom,
                                 bottomRightIntersectBf, topRightIntersectBf,
                                 bottomRightSlopeFP, invBottomRightSlopeFP);

          Q_ASSERT(excluded >= 0 && excluded <= rowHeight);
          QScFixed coverage = rowHeight - excluded;
          buffer.addSpan(x, 1, QScFixedToInt(yFP),
                         QScFixedToInt(255 * coverage));
          ++x;
        }

        leftIntersectAf += topLeftSlopeFP;
        leftIntersectBf += bottomLeftSlopeFP;
        rightIntersectAf += topRightSlopeFP;
        rightIntersectBf += bottomRightSlopeFP;
        topLeftIntersectAf = leftIntersectAf;
        topRightIntersectAf = rightIntersectAf;

        yFP += QScFixedFactor;
        rowTop = yFP;
      }
    } else { // aliased
      int iTop = int(top.y() + 0.5f);
      int iLeft = left.y() < 0.5f ? -1 : int(left.y() - 0.5f);
      int iRight = right.y() < 0.5f ? -1 : int(right.y() - 0.5f);
      int iBottom = bottom.y() < 0.5f ? -1 : int(bottom.y() - 0.5f);
      int iMiddle = qMin(iLeft, iRight);

      QScFixed leftIntersectAf = qSafeFloatToQScFixed(
          top.x() + 0.5f + (iTop + 0.5f - top.y()) * topLeftSlope);
      QScFixed leftIntersectBf = qSafeFloatToQScFixed(
          left.x() + 0.5f + (iLeft + 1.5f - left.y()) * bottomLeftSlope);
      QScFixed rightIntersectAf = qSafeFloatToQScFixed(
          top.x() - 0.5f + (iTop + 0.5f - top.y()) * topRightSlope);
      QScFixed rightIntersectBf = qSafeFloatToQScFixed(
          right.x() - 0.5f + (iRight + 1.5f - right.y()) * bottomRightSlope);

      int ny;
      int y = iTop;
#define DO_SEGMENT(next, li, ri, ls, rs)                                       \
  ny = qMin(next + 1, d->clipRect.top());                                      \
  if (y < ny) {                                                                \
    li += ls * (ny - y);                                                       \
    ri += rs * (ny - y);                                                       \
    y = ny;                                                                    \
  }                                                                            \
  if (next > d->clipRect.bottom())                                             \
    next = d->clipRect.bottom();                                               \
  for (; y <= next; ++y) {                                                     \
    const int x1 = qMax(QScFixedToInt(li), d->clipRect.left());                \
    const int x2 = qMin(QScFixedToInt(ri), d->clipRect.right());               \
    if (x2 >= x1)                                                              \
      buffer.addSpan(x1, x2 - x1 + 1, y, 255);                                 \
    li += ls;                                                                  \
    ri += rs;                                                                  \
  }

      DO_SEGMENT(iMiddle, leftIntersectAf, rightIntersectAf, topLeftSlopeFP,
                 topRightSlopeFP)
      DO_SEGMENT(iRight, leftIntersectBf, rightIntersectAf, bottomLeftSlopeFP,
                 topRightSlopeFP)
      DO_SEGMENT(iLeft, leftIntersectAf, rightIntersectBf, topLeftSlopeFP,
                 bottomRightSlopeFP);
      DO_SEGMENT(iBottom, leftIntersectBf, rightIntersectBf, bottomLeftSlopeFP,
                 bottomRightSlopeFP);
#undef DO_SEGMENT
    }
  }
}
