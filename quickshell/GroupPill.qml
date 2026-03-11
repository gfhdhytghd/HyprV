import QtQuick

Rectangle {
    id: pill

    property var shellRoot: null
    default property alias contentData: contentRow.data

    radius: 10
    color: shellRoot ? shellRoot.moduleBackground : "#303030"
    implicitWidth: contentRow.implicitWidth
    implicitHeight: 37
    width: implicitWidth
    height: implicitHeight

    Row {
        id: contentRow

        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        spacing: 0
    }
}
