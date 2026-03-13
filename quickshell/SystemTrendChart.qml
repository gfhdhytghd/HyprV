import QtQuick

Item {
    id: root

    property var shellRoot: null
    property var samples: []
    property real maxValue: 100
    property bool autoScale: false
    property real lineWidth: 2.25
    property int gridLineCount: 4
    property real surfaceOpacity: 0.82
    property color accentColor: shellRoot ? shellRoot.systemChartAccent : "#b9782f"
    property color frameColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.12 : 0.10) : Qt.rgba(0.2, 0.2, 0.2, 0.10)
    property color gridColor: shellRoot ? shellRoot.withAlpha(shellRoot.primaryText, shellRoot.darkMode ? 0.10 : 0.08) : Qt.rgba(0.2, 0.2, 0.2, 0.08)
    property color panelFill: shellRoot
        ? (shellRoot.darkMode
            ? shellRoot.withAlpha("#ffffff", 0.055)
            : shellRoot.withAlpha("#ffffff", 0.032))
        : Qt.rgba(0.2, 0.2, 0.2, 0.032)
    property color overlayFill: shellRoot
        ? (shellRoot.darkMode
            ? shellRoot.withAlpha("#ffffff", 0.018)
            : "transparent")
        : "transparent"
    property color areaTopColor: shellRoot ? shellRoot.withAlpha(accentColor, shellRoot.darkMode ? 0.18 : 0.14) : Qt.rgba(0.73, 0.47, 0.18, 0.14)
    property color areaBottomColor: shellRoot ? shellRoot.withAlpha(accentColor, 0.02) : Qt.rgba(0.73, 0.47, 0.18, 0.02)

    readonly property real resolvedMaxValue: {
        let upper = autoScale ? 0 : Math.max(1, Number(maxValue) || 1);
        const values = Array.isArray(samples) ? samples : [];
        for (let i = 0; i < values.length; i++) {
            upper = Math.max(upper, Math.max(0, Number(values[i]) || 0));
        }
        if (autoScale) {
            upper *= 1.15;
        }
        return Math.max(1, upper);
    }

    implicitWidth: 280
    implicitHeight: 116

    Rectangle {
        anchors.fill: parent
        radius: 10
        opacity: root.surfaceOpacity
        color: root.panelFill
        border.width: 1
        border.color: root.frameColor
        antialiasing: true

        Rectangle {
            anchors.fill: parent
            radius: parent.radius
            color: root.overlayFill
            border.width: 0
            antialiasing: true
            visible: color.a > 0
        }
    }

    Canvas {
        id: chartCanvas

        anchors.fill: parent
        anchors.margins: 2
        antialiasing: true
        renderStrategy: Canvas.Cooperative

        onPaint: {
            const ctx = getContext("2d");
            ctx.clearRect(0, 0, width, height);

            const values = Array.isArray(root.samples) ? root.samples : [];
            const left = 10;
            const right = 10;
            const top = 10;
            const bottom = 10;
            const chartWidth = Math.max(1, width - left - right);
            const chartHeight = Math.max(1, height - top - bottom);

            ctx.strokeStyle = root.gridColor;
            ctx.lineWidth = 1;
            for (let i = 0; i < root.gridLineCount; i++) {
                const ratio = root.gridLineCount <= 1 ? 0 : i / (root.gridLineCount - 1);
                const y = top + chartHeight * ratio;
                ctx.beginPath();
                ctx.moveTo(left, y);
                ctx.lineTo(left + chartWidth, y);
                ctx.stroke();
            }

            if (!values.length) {
                return;
            }

            const maxValue = Math.max(1, root.resolvedMaxValue);
            const stepX = values.length > 1 ? chartWidth / (values.length - 1) : 0;
            const points = [];
            for (let i = 0; i < values.length; i++) {
                const sample = Math.max(0, Math.min(maxValue, Number(values[i]) || 0));
                const x = left + stepX * i;
                const y = top + chartHeight - sample / maxValue * chartHeight;
                points.push({
                    x: x,
                    y: y
                });
            }

            if (points.length === 1) {
                points.push({
                    x: left + chartWidth,
                    y: points[0].y
                });
            }

            const gradient = ctx.createLinearGradient(0, top, 0, top + chartHeight);
            gradient.addColorStop(0, root.areaTopColor);
            gradient.addColorStop(1, root.areaBottomColor);

            ctx.beginPath();
            ctx.moveTo(points[0].x, top + chartHeight);
            for (let i = 0; i < points.length; i++) {
                ctx.lineTo(points[i].x, points[i].y);
            }
            ctx.lineTo(points[points.length - 1].x, top + chartHeight);
            ctx.closePath();
            ctx.fillStyle = gradient;
            ctx.fill();

            ctx.beginPath();
            ctx.moveTo(points[0].x, points[0].y);
            for (let i = 1; i < points.length; i++) {
                ctx.lineTo(points[i].x, points[i].y);
            }
            ctx.lineWidth = root.lineWidth;
            ctx.lineJoin = "round";
            ctx.lineCap = "round";
            ctx.strokeStyle = root.accentColor;
            ctx.stroke();

            const lastPoint = points[points.length - 1];
            ctx.beginPath();
            ctx.arc(lastPoint.x, lastPoint.y, 3, 0, Math.PI * 2, false);
            ctx.fillStyle = root.accentColor;
            ctx.fill();
        }
    }

    onSamplesChanged: chartCanvas.requestPaint()
    onResolvedMaxValueChanged: chartCanvas.requestPaint()
    onWidthChanged: chartCanvas.requestPaint()
    onHeightChanged: chartCanvas.requestPaint()
    onAccentColorChanged: chartCanvas.requestPaint()
    onFrameColorChanged: chartCanvas.requestPaint()
    onGridColorChanged: chartCanvas.requestPaint()
    onPanelFillChanged: chartCanvas.requestPaint()
    onOverlayFillChanged: chartCanvas.requestPaint()
    onAreaTopColorChanged: chartCanvas.requestPaint()
    onAreaBottomColorChanged: chartCanvas.requestPaint()
}
