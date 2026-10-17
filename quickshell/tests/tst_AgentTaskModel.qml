import QtQuick
import QtTest
import ".."

TestCase {
    id: test
    name: "AgentTaskModel"
    when: windowShown
    width: 300
    height: 300
    property int created: 0
    AgentTaskModel { id: rows }
    ListView {
        id: view
        width: 280
        height: 280
        model: rows
        delegate: Item {
            required property var model
            readonly property string sessionId: model.id
            readonly property string taskStatus: model.status
            width: 100
            height: 40
            Component.onCompleted: test.created++
        }
    }
    function test_progress_and_reordering_keep_delegates() {
        rows.sync([{id: "a", status: "running", progress: 0.2}, {id: "b", status: "completed"}]);
        tryCompare(test, "created", 2);
        rows.sync([{id: "a", status: "running", progress: 0.8}, {id: "b", status: "completed"}]);
        wait(10);
        compare(created, 2);
        rows.sync([{id: "b", status: "completed"}, {id: "a", status: "processing"}]);
        view.forceLayout();
        wait(30);
        compare(created, 2);
        compare(view.itemAtIndex(0).sessionId, "b");
        compare(view.itemAtIndex(1).taskStatus, "processing");
        rows.sync([{id: "b", status: "completed"}]);
        compare(rows.count, 1);
    }
}
