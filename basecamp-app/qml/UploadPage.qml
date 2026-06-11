import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

Item {
    required property var module

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 20
        spacing: 12

        Label { text: qsTr("Upload a document"); font.bold: true; font.pixelSize: 16 }

        RowLayout {
            Label { text: qsTr("File:"); Layout.preferredWidth: 80 }
            TextField {
                id: filePath
                Layout.fillWidth: true
                placeholderText: qsTr("Select a file...")
                readOnly: true
            }
            Button {
                text: qsTr("Browse")
                onClicked: fileDialog.open()
            }
        }

        RowLayout {
            Label { text: qsTr("Title:"); Layout.preferredWidth: 80 }
            TextField {
                id: titleField
                Layout.fillWidth: true
                placeholderText: qsTr("Document title")
            }
        }

        RowLayout {
            Label { text: qsTr("Description:"); Layout.preferredWidth: 80 }
            TextArea {
                id: descField
                Layout.fillWidth: true
                implicitHeight: 80
                placeholderText: qsTr("Brief description")
                wrapMode: TextArea.Wrap
            }
        }

        RowLayout {
            Label { text: qsTr("Tags:"); Layout.preferredWidth: 80 }
            TextField {
                id: tagsField
                Layout.fillWidth: true
                placeholderText: qsTr("Comma-separated tags (optional)")
            }
        }

        CheckBox {
            id: anchorCheck
            text: qsTr("Anchor on-chain after upload")
            checked: false
        }

        Button {
            text: module.uploading ? qsTr("Uploading...") : qsTr("Upload and Broadcast")
            enabled: filePath.text.length > 0 && !module.uploading
            Layout.fillWidth: true
            onClicked: {
                const tags = tagsField.text.split(",").map(t => t.trim()).filter(t => t.length > 0)
                module.uploadFile(
                    Qt.url(filePath.text),
                    titleField.text,
                    descField.text,
                    tags,
                    anchorCheck.checked
                )
            }
        }

        Label {
            visible: module.lastCid.length > 0
            text: qsTr("Last CID: ") + module.lastCid
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
        }

        Item { Layout.fillHeight: true }
    }

    FileDialog {
        id: fileDialog
        title: qsTr("Select document to upload")
        onAccepted: filePath.text = selectedFile
    }
}
