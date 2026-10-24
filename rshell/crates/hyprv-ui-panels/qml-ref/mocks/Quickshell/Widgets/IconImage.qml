import QtQuick

// Reference-harness stand-in for Quickshell.Widgets.IconImage: an Image whose source
// size follows the item size (Quickshell renders icons at the item size × dpr).
Image {
    sourceSize.width: width
    sourceSize.height: height
    fillMode: Image.PreserveAspectFit
}
