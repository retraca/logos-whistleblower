import QtQuick
import QtQuick.Controls
import com.logos.whistleblower 1.0

ApplicationWindow {
    id: root
    title: qsTr("Whistleblower")
    width: 600
    height: 700
    visible: true

    WhistleblowerModule {
        id: whistleblower
        onUploadFinished: (cid, success, error) => {
            if (success) {
                statusBar.text = qsTr("Published. CID: ") + cid
            } else {
                statusBar.text = qsTr("Error: ") + error
            }
        }
    }

    TabBar {
        id: tabBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        TabButton { text: qsTr("Upload") }
        TabButton { text: qsTr("Index") }
    }

    StackLayout {
        anchors.top: tabBar.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: statusBar.top
        currentIndex: tabBar.currentIndex

        UploadPage {
            module: whistleblower
        }

        IndexPage {
            module: whistleblower
        }
    }

    Label {
        id: statusBar
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        padding: 8
        text: whistleblower.status
        wrapMode: Text.WordWrap
    }
}
