#pragma once
// Copyright (C) 2016 The Qt Company Ltd.
// SPDX-License-Identifier: LGPL-3.0-only
// Standalone port of Qt 6.12 qdistancefield.cpp raster kernels. Qt types below
// are replaced by standard C++ storage; this file neither links nor calls Qt.
#include <vector>
#include <memory>
#include <cmath>
#include <cstdint>
#include <algorithm>
#include <cassert>
#include <climits>
using qint32=int32_t; using qint64=int64_t; using quint32=uint32_t;
using quint16=uint16_t; using qreal=double; using uchar=unsigned char;
#define Q_ASSERT(x) assert(x)
#define Q_UNUSED(x) (void)x
using std::abs;
template<class T> T qMax(T a,T b){return std::max(a,b);}
template<class T> T qMin(T a,T b){return std::min(a,b);}
template<class T> T qBound(T a,T b,T c){return std::clamp(b,a,c);}
template<class T> void qSwap(T&a,T&b){std::swap(a,b);}
inline double qSqrt(double x){return std::sqrt(x);}
inline int qRound(double x){return x>=0?int(x+0.5):int(x-0.5);}
inline double qDegreesToRadians(double x){return x*3.14159265358979323846/180.;}
inline double qCos(double x){return std::cos(x);} inline double qSin(double x){return std::sin(x);}
struct QPoint { int xx=0, yy=0; QPoint()=default; QPoint(int x,int y):xx(x),yy(y){} int x()const{return xx;} int y()const{return yy;} int&rx(){return xx;} int&ry(){return yy;}
QPoint operator+(QPoint b)const{return QPoint(xx+b.xx,yy+b.yy);} QPoint operator-(QPoint b)const{return QPoint(xx-b.xx,yy-b.yy);} bool operator==(QPoint b)const{return xx==b.xx&&yy==b.yy;} bool operator!=(QPoint b)const{return !(*this==b);} };
template<class T,int N=0> struct QVarLengthArray:std::vector<T>{using std::vector<T>::vector; void append(const T&v){this->push_back(v);} bool isEmpty()const{return this->empty();} };
template<class T> struct QScopedArrayPointer:std::unique_ptr<T[]>{using std::unique_ptr<T[]>::unique_ptr;T*data(){return this->get();}};

#include <cstring>
#include <cstdlib>
#include <utility>
#define QT_BEGIN_NAMESPACE
#define QT_END_NAMESPACE
#define QT_NO_DEBUG
#define Q_ASSERT_X(a,b,c) assert(a)
#define Q_DECLARE_TYPEINFO(a,b)
template<class T> class QTypeInfo;
template<class T> T qAbs(T x){return std::abs(x);}
using uint=unsigned int;
template<class T> using QList=std::vector<T>;
template<class T> struct QDataBuffer:std::vector<T>{explicit QDataBuffer(size_t capacity){this->reserve(capacity);}void add(T v){this->push_back(v);}void reset(){this->clear();}bool isEmpty()const{return this->empty();}T&last(){return this->back();}T&first(){return this->front();}};
struct QPainterPath{enum ElementType{MoveToElement,LineToElement,CurveToElement,CurveToDataElement};};
struct QVectorPath{const double*p;const QPainterPath::ElementType*e;int n;uint hints()const{return 0;}int elementCount()const{return n;}const double*points()const{return p;}const QPainterPath::ElementType*elements()const{return e;}};
struct QTransform{double x,y;void map(double px,double py,double*ox,double*oy)const{*ox=px+x;*oy=py+y;}};
