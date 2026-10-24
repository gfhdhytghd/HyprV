pragma Singleton
import QtQuick
import ParityHost

// services.tray: [{id, title, icon, tooltipTitle, tooltipDescription, hasMenu, onlyMenu,
//                  menu: {entries: [{text, icon, isSeparator, enabled, buttonType,
//                                    checkState (0/2), entries: [...]}]}}]
// icons may be "@asset:<file>" (fixture asset), a theme icon name (tray item) or a URL.
QtObject {
    id: tray

    property Component entryComponent: Component { MenuEntry {} }

    function asset(icon) {
        const value = String(icon || "");
        return value.startsWith("@asset:") ? "file://" + Harness.assets + "/" + value.slice(7) : value;
    }

    function buildEntry(data) {
        const entry = entryComponent.createObject(tray, {
            text: data.text || "", icon: asset(data.icon), isSeparator: !!data.isSeparator,
            enabled: data.enabled !== false, buttonType: data.buttonType || 0,
            checkState: data.checkState || 0
        });
        entry.entries = (data.entries || []).map(child => buildEntry(child));
        return entry;
    }

    readonly property var items: ({
        values: ParityState.service("tray", []).map(item => Object.assign({
            id: "", title: "", icon: "", tooltipTitle: "", tooltipDescription: "",
            hasMenu: false, onlyMenu: false, status: 1,
            activate: function () { console.log("parity: tray activate", item.id); },
            secondaryActivate: function () {},
            scroll: function (delta, horizontal) {}
        }, item, {
            icon: asset(item.icon),
            hasMenu: !!item.menu || !!item.hasMenu,
            menu: item.menu ? buildEntry(item.menu) : null
        }))
    })
}
