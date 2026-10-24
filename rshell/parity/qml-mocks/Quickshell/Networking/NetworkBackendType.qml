pragma Singleton
import QtQuick

QtObject {
    enum Enum { None, NetworkManager }
    function toString(type) { return type === 1 ? "NetworkManager" : "None"; }
}
