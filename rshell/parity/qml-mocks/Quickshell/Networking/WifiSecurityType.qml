pragma Singleton
import QtQuick

// Enum order from quickshell-network.qmltypes; toString labels from the installed
// quickshell binary's UTF-16 string table (StaticWep "WEP" and Unknown assumed).
QtObject {
    enum Enum { Wpa3SuiteB192, Sae, Wpa2Eap, Wpa2Psk, WpaEap, WpaPsk, StaticWep, DynamicWep, Leap, Owe, Open, Unknown }
    readonly property var labels: ["WPA3 Suite B 192-bit", "WPA3", "WPA2 Enterprise", "WPA2", "WPA Enterprise",
        "WPA", "WEP", "Dynamic WEP", "LEAP", "OWE", "Open", "Unknown"]
    function toString(type) { return labels[type] ?? "Unknown"; }
}
