import QtQuick

Item {
    id: root

    property string corner: "topLeft"
    property color shadeColor: Qt.rgba(0, 0, 0, 0.9)

    implicitWidth: 24
    implicitHeight: 24

    Canvas {
        id: shadeCanvas

        anchors.fill: parent
        antialiasing: true
        contextType: "2d"
        renderStrategy: Canvas.Cooperative

        onPaint: {
            const ctx = getContext("2d");
            if (ctx.resetTransform) {
                ctx.resetTransform();
            } else if (ctx.setTransform) {
                ctx.setTransform(1, 0, 0, 1, 0, 0);
            }
            ctx.clearRect(0, 0, width, height);

            const w = width;
            const h = height;
            const radius = Math.min(w, h);

            ctx.save();
            switch (root.corner) {
            case "topRight":
                ctx.translate(w, 0);
                ctx.scale(-1, 1);
                break;
            case "bottomLeft":
                ctx.translate(0, h);
                ctx.scale(1, -1);
                break;
            case "bottomRight":
                ctx.translate(w, h);
                ctx.scale(-1, -1);
                break;
            default:
                break;
            }

            ctx.beginPath();
            ctx.moveTo(0, 0);
            ctx.lineTo(w, 0);
            ctx.arc(w, h, radius, Math.PI * 1.5, Math.PI, true);
            ctx.lineTo(0, 0);
            ctx.closePath();
            ctx.fillStyle = root.shadeColor;
            ctx.fill();
            ctx.restore();
        }
    }

    onCornerChanged: shadeCanvas.requestPaint()
    onShadeColorChanged: shadeCanvas.requestPaint()
    onWidthChanged: shadeCanvas.requestPaint()
    onHeightChanged: shadeCanvas.requestPaint()

    Component.onCompleted: shadeCanvas.requestPaint()
}
