import QtQuick
import "../../../../quickshell" as Legacy
Item {
    width: 180; height: 120
    function applyProps(props) {}
    property var values: [12,38,74,92,66,41,58,80,95,61,33,47,70,52,28,15]
    property color accent: Harness.dark ? "#89b4fa" : "#407cdd"
    property color ink: Harness.dark ? "#cdd6f4" : "#2b2b2c"
    property color muted: Qt.rgba(ink.r,ink.g,ink.b,Harness.dark ? .68 : .62)
    Repeater {
        model: 6
        Legacy.AudioSpectrum {
            required property int index
            x: index < 4 ? 10 + index*40 : 10+(index-4)*90
            y: index < 4 ? 10 : 50
            width: index < 4 ? 22 : 70; height: index < 4 ? 22 : 56
            barCount: index < 4 ? 6 : 8
            minimumBarRatio: index < 4 ? .18 : .10
            amplitudeGain: index < 4 ? 2.05 : 1.75
            active: index === 0 || index === 3 || index === 4
            values: index === 3 ? [] : parent.values
            barColor: parent.accent; peakColor: parent.ink; quietColor: parent.muted
            Component.onCompleted: {
                // Fixed-phase independent raster probe, no original source edit.
                for(let i=0;i<data.length;i++) if(data[i].interval !== undefined) data[i].running=false;
                phase = index === 1 ? 0 : 6.44;
                requestPaint();
            }
        }
    }
}
