pragma Singleton
import QtQuick

// Quickshell.Networking DeviceType (quickshell-network.qmltypes: None, Wifi, Wired).
QtObject {
    enum Enum { None, Wifi, Wired }
    function toString(type) { return ["None", "Wifi", "Wired"][type] ?? "None"; }
}
