import QtQuick

Rectangle {
    id: pill

    property var shellRoot: null
    default property alias contentData: contentRow.data

    radius: 24
    color: shellRoot ? shellRoot.moduleBackground : "#303030"
    implicitWidth: contentRow.implicitWidth
    implicitHeight: 38
    width: implicitWidth
    height: implicitHeight

    Row {
        id: contentRow

        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        spacing: 0
    }
}
