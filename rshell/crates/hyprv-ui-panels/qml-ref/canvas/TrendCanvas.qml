// Canvas.onPaint copied verbatim from quickshell/SystemTrendChart.qml (read-only source),
// with the colours passed in explicitly instead of through shellRoot.
import QtQuick
Canvas {
    id: chartCanvas
    property var samples: []
    property real maxValue: 100
    property bool autoScale: false
    property real lineWidth: 2.25
    property int gridLineCount: 4
    property color accentColor: "#b9782f"
    property color gridColor: Qt.rgba(0.2, 0.2, 0.2, 0.08)
    property color areaTopColor: Qt.rgba(0.73, 0.47, 0.18, 0.14)
    property color areaBottomColor: Qt.rgba(0.73, 0.47, 0.18, 0.02)
    readonly property real resolvedMaxValue: {
        let upper = autoScale ? 0 : Math.max(1, Number(maxValue) || 1);
        const values = Array.isArray(samples) ? samples : [];
        for (let i = 0; i < values.length; i++)
            upper = Math.max(upper, Math.max(0, Number(values[i]) || 0));
        if (autoScale)
            upper *= 1.15;
        return Math.max(1, upper);
    }
    antialiasing: true
    renderStrategy: Canvas.Cooperative
    onPaint: {
        const ctx = getContext("2d");
        ctx.clearRect(0, 0, width, height);
        const values = Array.isArray(chartCanvas.samples) ? chartCanvas.samples : [];
        const left = 10; const right = 10; const top = 10; const bottom = 10;
        const chartWidth = Math.max(1, width - left - right);
        const chartHeight = Math.max(1, height - top - bottom);
        ctx.strokeStyle = chartCanvas.gridColor;
        ctx.lineWidth = 1;
        for (let i = 0; i < chartCanvas.gridLineCount; i++) {
            const ratio = chartCanvas.gridLineCount <= 1 ? 0 : i / (chartCanvas.gridLineCount - 1);
            const y = top + chartHeight * ratio;
            ctx.beginPath(); ctx.moveTo(left, y); ctx.lineTo(left + chartWidth, y); ctx.stroke();
        }
        if (!values.length) return;
        const maxValue = Math.max(1, chartCanvas.resolvedMaxValue);
        const stepX = values.length > 1 ? chartWidth / (values.length - 1) : 0;
        const points = [];
        for (let i = 0; i < values.length; i++) {
            const sample = Math.max(0, Math.min(maxValue, Number(values[i]) || 0));
            points.push({ x: left + stepX * i, y: top + chartHeight - sample / maxValue * chartHeight });
        }
        if (points.length === 1) points.push({ x: left + chartWidth, y: points[0].y });
        const gradient = ctx.createLinearGradient(0, top, 0, top + chartHeight);
        gradient.addColorStop(0, chartCanvas.areaTopColor);
        gradient.addColorStop(1, chartCanvas.areaBottomColor);
        ctx.beginPath();
        ctx.moveTo(points[0].x, top + chartHeight);
        for (let i = 0; i < points.length; i++) ctx.lineTo(points[i].x, points[i].y);
        ctx.lineTo(points[points.length - 1].x, top + chartHeight);
        ctx.closePath();
        ctx.fillStyle = gradient;
        ctx.fill();
        ctx.beginPath();
        ctx.moveTo(points[0].x, points[0].y);
        for (let i = 1; i < points.length; i++) ctx.lineTo(points[i].x, points[i].y);
        ctx.lineWidth = chartCanvas.lineWidth;
        ctx.lineJoin = "round";
        ctx.lineCap = "round";
        ctx.strokeStyle = chartCanvas.accentColor;
        ctx.stroke();
        const lastPoint = points[points.length - 1];
        ctx.beginPath();
        ctx.arc(lastPoint.x, lastPoint.y, 3, 0, Math.PI * 2, false);
        ctx.fillStyle = chartCanvas.accentColor;
        ctx.fill();
    }
}
