import QtQuick

// One delegate instance per model entry, with `modelData` as a required property.
QtObject {
    id: variants
    property var model: []
    default property Component delegate
    readonly property var instances: inst.objects

    property Instantiator inst: Instantiator {
        id: inst
        property var objects: []
        model: variants.model
        delegate: variants.delegate
        onObjectAdded: (index, object) => objects = objects.concat([object])
        onObjectRemoved: (index, object) => objects = objects.filter(o => o !== object)
    }
}
