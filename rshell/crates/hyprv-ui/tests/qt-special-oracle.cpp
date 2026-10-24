// Regenerate with Qt6Core; see qt-oracle.cpp.
#include <QEasingCurve>
#include <QPointF>
#include <cstdio>
int main(){
    puts("# Qt " QT_VERSION_STR " QEasingCurve custom parameters; name,t,value");
    auto dump=[](const char* name,QEasingCurve c){for(double t:{0.,0.01,0.125,0.25,0.499,0.5,0.75,0.875,0.99,1.})printf("%s,%.17g,%.17g\n",name,t,c.valueForProgress(t));};
    QEasingCurve back(QEasingCurve::OutBack);back.setOvershoot(2.);dump("Back2",back);
    QEasingCurve elastic(QEasingCurve::InOutElastic);elastic.setAmplitude(2.);elastic.setPeriod(0.4);dump("Elastic2",elastic);
    QEasingCurve bounce(QEasingCurve::OutInBounce);bounce.setAmplitude(0.3);dump("Bounce03",bounce);
    QEasingCurve bar(QEasingCurve::BezierSpline);bar.addCubicBezierSegment(QPointF(0.1,0.9),QPointF(0.1,1.05),QPointF(1,1));dump("BarBezier",bar);
    QEasingCurve multi(QEasingCurve::BezierSpline);multi.addCubicBezierSegment(QPointF(0.1,0.3),QPointF(0.3,0.6),QPointF(0.5,0.5));multi.addCubicBezierSegment(QPointF(0.6,0.4),QPointF(0.8,1.2),QPointF(1,1));dump("MultiBezier",multi);
}
