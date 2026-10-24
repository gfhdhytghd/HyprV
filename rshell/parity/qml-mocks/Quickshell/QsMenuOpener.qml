import QtQuick

// Tray menu model: menu.children from fixture tray item "menu" data.
QtObject {
    property var menu: null
    // Repeater model: the entry objects of the opened menu (Quickshell: UntypedObjectModel).
    readonly property var children: menu && menu.entries ? menu.entries : []
}
