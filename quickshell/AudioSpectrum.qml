import QtQuick

Canvas {
    id: spectrum

    property var values: []
    property int barCount: 14
    property bool active: false
    property color barColor: "#407cdd"
    property color peakColor: "#cdd6f4"
    property color quietColor: Qt.rgba(0.8, 0.8, 0.8, 0.24)
    property real minimumBarRatio: 0.12
    property real amplitudeGain: 1.7
    property real heightRangeScale: 0.65

    readonly property bool hasEnergy: {
        if (!values || values.length === 0) {
            return false;
        }
        for (let i = 0; i < values.length; i++) {
            if (Number(values[i]) > 0) {
                return true;
            }
        }
        return false;
    }

    property real phase: 0

    function normalizedSample(index) {
        let ratio = minimumBarRatio;
        if (!active || !hasEnergy || !values || values.length === 0) {
            ratio = minimumBarRatio + 0.34 * Math.pow((Math.sin(phase + index * 0.72) + 1) / 2, 1.45);
        } else {
            const sourceIndex = Math.min(values.length - 1, Math.floor(index * values.length / Math.max(1, barCount)));
            const rawValue = Math.max(0, Number(values[sourceIndex]) || 0);
            ratio = Math.max(minimumBarRatio, Math.min(1, Math.pow(Math.min(1, rawValue / 100), 0.62) * amplitudeGain));
        }
        return minimumBarRatio + (Math.min(1, ratio) - minimumBarRatio) * heightRangeScale;
    }

    function roundedRect(ctx, x, y, width, height, radius) {
        const r = Math.max(0, Math.min(radius, width / 2, height / 2));
        ctx.beginPath();
        ctx.moveTo(x + r, y);
        ctx.lineTo(x + width - r, y);
        ctx.quadraticCurveTo(x + width, y, x + width, y + r);
        ctx.lineTo(x + width, y + height - r);
        ctx.quadraticCurveTo(x + width, y + height, x + width - r, y + height);
        ctx.lineTo(x + r, y + height);
        ctx.quadraticCurveTo(x, y + height, x, y + height - r);
        ctx.lineTo(x, y + r);
        ctx.quadraticCurveTo(x, y, x + r, y);
        ctx.closePath();
    }

    Timer {
        interval: 66
        repeat: true
        running: spectrum.visible && (!spectrum.active || !spectrum.hasEnergy)
        onTriggered: {
            spectrum.phase += 0.28;
            spectrum.requestPaint();
        }
    }

    onPaint: {
        const ctx = getContext("2d");
        ctx.clearRect(0, 0, width, height);

        const count = Math.max(1, barCount);
        const gap = Math.max(2, Math.min(4, width / 36));
        const barWidth = Math.max(2, (width - gap * (count - 1)) / count);
        const centerY = height / 2;
        const maxBarHeight = Math.max(2, height * 0.98);

        for (let i = 0; i < count; i++) {
            const ratio = normalizedSample(i);
            const barHeight = Math.max(3, maxBarHeight * ratio);
            const x = i * (barWidth + gap);
            const y = centerY - barHeight / 2;
            const isPeak = active && hasEnergy && ratio > 0.74;

            ctx.fillStyle = active ? (isPeak ? peakColor : barColor) : quietColor;
            roundedRect(ctx, x, y, barWidth, barHeight, barWidth / 2);
            ctx.fill();
        }
    }

    onValuesChanged: requestPaint()
    onActiveChanged: requestPaint()
    onBarCountChanged: requestPaint()
    onBarColorChanged: requestPaint()
    onPeakColorChanged: requestPaint()
    onQuietColorChanged: requestPaint()
    onAmplitudeGainChanged: requestPaint()
    onHeightRangeScaleChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
}
