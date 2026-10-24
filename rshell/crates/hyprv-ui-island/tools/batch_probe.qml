import QtQuick
Item {
    width: 640; height: 200
    function applyProps(props) {}
    Item {
        x:124; y:10; width:392; height:180; clip:true
        Rectangle { x:174; y:119; width:44; height:44; radius:22; color:"#b889b4fa" }
        Item {
            x:236; y:121; width:40; height:40; scale:1.06
            Rectangle { anchors.fill:parent; radius:20; color:"#211e1e2e"; border.width:1; border.color:"#1dcdd6f4" }
        }
    }
}
