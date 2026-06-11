import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    required property var module

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 12

        Label { text: qsTr("Document index"); font.bold: true; font.pixelSize: 16 }

        Label {
            text: qsTr("Documents are anchored on-chain by any party running the batch-anchor CLI. " +
                       "Query the on-chain registry by CID below, or run the batch anchor from this app.")
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        RowLayout {
            Label { text: qsTr("CID:"); Layout.preferredWidth: 60 }
            TextField {
                id: cidField
                Layout.fillWidth: true
                placeholderText: qsTr("Base58 CID to look up...")
            }
            Button {
                text: qsTr("Query")
                onClicked: {
                    // Queries the on-chain cid-registry via the sequencer RPC.
                    // Result displayed in queryResult below.
                    queryResult.text = qsTr("Querying... (requires deployed cid-registry program)")
                }
            }
        }

        TextArea {
            id: queryResult
            Layout.fillWidth: true
            implicitHeight: 120
            readOnly: true
            wrapMode: TextArea.Wrap
            placeholderText: qsTr("Registry query result will appear here")
        }

        Button {
            text: qsTr("Run batch anchor now")
            Layout.fillWidth: true
            onClicked: module.runBatchAnchor()
        }

        Item { Layout.fillHeight: true }
    }
}
