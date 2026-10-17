import QtQml.Models

// Preserve delegates and scroll position while progress rings update separately.
ListModel {
    function sync(sessions) {
        const items = Array.isArray(sessions) ? sessions : [];
        const fields = ["id", "source", "source_label", "title", "project", "status", "current_tool", "detail"];
        for (let i = 0; i < items.length; ++i) {
            const row = {};
            for (const field of fields) row[field] = String(items[i][field] ?? "");
            let found = i;
            while (found < count && get(found).id !== row.id) ++found;
            if (found === count) insert(i, row);
            else {
                if (found !== i) move(found, i, 1);
                for (const field of fields)
                    if (get(i)[field] !== row[field]) setProperty(i, field, row[field]);
            }
        }
        if (count > items.length) remove(items.length, count - items.length);
    }
}
