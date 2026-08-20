import QtQuick
import Quickshell

QtObject {
    id: root

    readonly property string home: Quickshell.env("HOME")
    readonly property string xdgRuntimeDir: Quickshell.env("XDG_RUNTIME_DIR")
    readonly property string configDir: home + "/.config/HyprV/quickshell/hypr-comp"
    readonly property string qsDir: configDir + "/scripts/quickshell"
    readonly property string serpantinumDir: configDir

    readonly property string cacheDir: home + "/.cache/hyprv/hypr-comp"
    readonly property string stateDir: home + "/.local/state/hyprv/hypr-comp"
    readonly property string runDir: (xdgRuntimeDir !== "" ? xdgRuntimeDir : "/tmp") + "/hyprv-hypr-comp"
    readonly property string logDir: runDir + "/logs"

    function ensureDir(path) {
        Quickshell.execDetached(["mkdir", "-p", path]);
        return path;
    }

    function getCacheDir(widgetName) {
        return ensureDir(cacheDir + "/" + widgetName);
    }

    function getStateDir(widgetName) {
        return ensureDir(stateDir + "/" + widgetName);
    }

    function getRunDir(widgetName) {
        return ensureDir(runDir + "/" + widgetName);
    }

    function getLogDir(widgetName) {
        return ensureDir(logDir + "/" + widgetName);
    }
}
