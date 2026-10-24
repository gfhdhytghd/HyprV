// Independent build-time Qt geometry oracle. Not linked into the Rust runtime.
#include <QtQuick/private/qsgdefaultinternalrectanglenode_p.h>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <cstdio>
struct Vertex{float x,y;unsigned char rgba[4];float dx,dy;};
int main(int argc,char**argv){double width=argc>1?atof(argv[1]):223.375f;QSGDefaultInternalRectangleNode node;node.setRect(QRectF(0,0,width,argc>3?atof(argv[3]):38));node.setRadius(argc>4?atof(argv[4]):19);node.setColor(QColor(30,30,46));node.setAntialiasing(true);if(argc>2){node.setPenWidth(atof(argv[2]));node.setPenColor(QColor(205,214,244,29));}node.update();auto*g=node.geometry();QJsonArray vertices,indices;auto*v=static_cast<Vertex*>(g->vertexData());for(int i=0;i<g->vertexCount();i++)vertices.append(QJsonArray{v[i].x,v[i].y,v[i].dx,v[i].dy,int(v[i].rgba[0]),int(v[i].rgba[1]),int(v[i].rgba[2]),int(v[i].rgba[3])});for(int i=0;i<g->indexCount();i++)indices.append(g->indexDataAsUShort()[i]);auto data=QJsonDocument(QJsonObject{{"vertices",vertices},{"indices",indices}}).toJson();fwrite(data.data(),1,data.size(),stdout);}
