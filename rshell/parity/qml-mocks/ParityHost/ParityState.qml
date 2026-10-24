pragma Singleton
import QtQuick

// Shared state of the mock Quickshell modules, fed from the fixture JSON.
//   services:  data for Hyprland/Mpris/Pipewire/... singletons (see README)
//   processes: canned outputs for Process commands (nothing is executed)
//   files:     contents for FileView paths
//   env:       values returned by Quickshell.env()
QtObject {
    id: state

    readonly property var fixture: Harness.fixture
    // "@ASSETS@" in service strings (laptop MPRIS art URLs) -> the fixture asset dir.
    function substituted(value) {
        const text = JSON.stringify(value === undefined ? null : value);
        return text.indexOf("@ASSETS@") < 0 ? value : JSON.parse(text.replace(/@ASSETS@/g, Harness.assets));
    }
    property var services: substituted(fixture.services || ({}))
    property var processes: fixture.processes || []
    property var files: fixture.files || ({})
    property var env: fixture.env || ({})
    property date now: fixture.now ? new Date(fixture.now) : new Date(2026, 9, 23, 9, 41)

    // Every mock PanelWindow / IpcHandler registers here so the scene and the
    // fixture steps can find them.
    property var windows: []
    // Scene item that mock window contents start under (see scenes/desktop.qml).
    property Item earlyParent: null
    property var ipcHandlers: []
    property var unmatched: ({})
    property var sequenceCounts: ({})

    function registerWindow(w) { windows = windows.concat([w]); }
    function unregisterWindow(w) { windows = windows.filter(x => x !== w); }
    function registerIpc(h) { ipcHandlers = ipcHandlers.concat([h]); }

    function service(path, fallback) {
        let value = services;
        for (const key of path.split(".")) {
            if (value === undefined || value === null)
                return fallback;
            value = value[key];
        }
        return value === undefined ? fallback : value;
    }

    // Merge a partial services object (fixture step "services").
    function mergeServices(patch) {
        patch = substituted(patch);
        const merged = JSON.parse(JSON.stringify(services));
        for (const key in patch)
            merged[key] = patch[key];
        services = merged;
    }

    function envValue(name) {
        if (env[name] !== undefined)
            return String(env[name]);
        if (name === "HOME")
            return Harness.home;
        return "";
    }

    // Find the canned result for a command: the first entry whose "match"
    // substring occurs in the space-joined command line.
    function processFor(command) {
        const line = (command || []).join(" ");
        for (let i = 0; i < processes.length; ++i) {
            const entry = processes[i];
            if (line.indexOf(entry.match) < 0)
                continue;
            // Optional "sequence": the n-th start of this entry answers with
            // sequence[min(n, last)] merged over the entry (polled commands).
            if (!entry.sequence)
                return entry;
            const n = sequenceCounts[i] || 0;
            const next = Object.assign({}, sequenceCounts);
            next[i] = n + 1;
            sequenceCounts = next;
            return Object.assign({}, entry, entry.sequence[Math.min(n, entry.sequence.length - 1)]);
        }
        if (!unmatched[line]) {
            const next = Object.assign({}, unmatched);
            next[line] = true;
            unmatched = next;
            console.log("parity: unmatched process (exit 1, no output):", line);
        }
        return null;
    }

    function fileFor(path) {
        for (const key in files) {
            if (String(path).endsWith(key))
                return files[key];
        }
        return null;
    }

    function callIpc(target, fn, args) {
        for (const h of ipcHandlers) {
            if (h.target === target && h.enabled !== false && typeof h[fn] === "function")
                return h[fn].apply(h, args || []);
        }
        console.warn("parity: no IpcHandler", target, fn);
        return undefined;
    }
}
