pragma Singleton
import QtQuick
import ParityHost

// services.upower: { onBattery: false, displayDevice: {isPresent, percentage, state, ...} }
QtObject {
    readonly property bool onBattery: ParityState.service("upower.onBattery", false)
    readonly property var displayDevice: ParityState.service("upower.displayDevice", { isPresent: false, ready: true, percentage: 0, state: 0 })
    readonly property var devices: ({ values: [] })
}
