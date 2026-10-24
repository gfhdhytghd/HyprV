pragma Singleton
import QtQuick
import ParityHost

// services.powerProfile: "power-saver" | "balanced" | "performance"
QtObject {
    readonly property int profile: ({ "power-saver": 0, "balanced": 1, "performance": 2 })[ParityState.service("powerProfile", "balanced")] ?? 1
    readonly property bool hasPerformanceProfile: true
}
