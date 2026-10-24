// Regenerate: c++ -fPIC -pie qt-oracle.cpp $(pkg-config --cflags --libs Qt6Core) -o qt-oracle
// ./qt-oracle > qt-easing.csv
#include <QEasingCurve>
#include <QMetaEnum>
#include <cstdio>
#include <string>
int main() {
    puts("# Qt " QT_VERSION_STR " QEasingCurve default parameters; family,direction,t,value");
    const char *families[]={"Quad","Cubic","Quart","Quint","Sine","Expo","Circ","Back","Elastic","Bounce"};
    const char *directions[]={"In","Out","InOut","OutIn"};
    auto types=QMetaEnum::fromType<QEasingCurve::Type>();
    for(auto f:families) for(auto d:directions) {
        auto name=std::string(d)+f;
        QEasingCurve c(static_cast<QEasingCurve::Type>(types.keyToValue(name.c_str())));
        for(double t:{0.,0.01,0.125,0.25,0.499,0.5,0.75,0.875,0.99,1.})
            printf("%s,%s,%.17g,%.17g\n",f,d,t,c.valueForProgress(t));
    }
}
