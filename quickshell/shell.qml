// switchout — Quickshell menu, embedded in the binary and opened with `switchout menu`.
// All the logic lives in the `switchout` binary; this file only draws and calls it.

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland

ShellRoot {
    id: root

    readonly property string bin: Quickshell.env("SWITCHOUT_BIN") || "switchout"
    readonly property var modes: [
        { id: "mirror", label: "Mirror", hint: "Same picture everywhere" },
        { id: "extend", label: "Extend", hint: "More desktop space" },
        { id: "external-only", label: "External only", hint: "Laptop screen off" },
        { id: "internal-only", label: "Internal only", hint: "External screens off" },
    ]

    property var status: null
    readonly property var externals: status ? status.screens.filter(s => !s.primary) : []
    readonly property string primaryName: {
        const p = status ? status.screens.find(s => s.primary) : null;
        return p ? p.name : "";
    }
    // 0 = all external screens, n = externals[n - 1]
    property int target: 0
    property int selected: 0
    property string error: ""
    property bool busy: false

    QtObject {
        id: theme
        readonly property color backdrop: "#99000000"
        readonly property color surface: "#211c21"
        readonly property color surfaceVariant: "#372f37"
        readonly property color outline: "#736373"
        readonly property color text: "#f3f2f3"
        readonly property color textDim: "#b6afb6"
        readonly property color accent: "#e467e4"
        readonly property color accentText: "#0b070b"
        readonly property color error: "#fd4663"
    }

    function apply(index) {
        if (busy || !status || status.error) return;
        selected = index;
        const args = [bin, "apply", modes[index].id];
        if (target > 0) args.push("--output", externals[target - 1].name);
        busy = true;
        error = "";
        applyProc.command = args;
        applyProc.running = true;
    }

    Process {
        id: statusProc
        command: [root.bin, "status", "--json"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    root.status = JSON.parse(text);
                    const i = root.modes.findIndex(m => m.id === root.status.mode);
                    if (i >= 0) root.selected = i;
                    if (root.status.error) root.error = root.status.error;
                } catch (e) {
                    root.error = "Could not read `switchout status`";
                }
            }
        }
        stderr: StdioCollector { id: statusErr }
        onExited: code => {
            if (code !== 0) root.error = statusErr.text.trim() || `${root.bin} not found`;
        }
    }

    Process {
        id: applyProc
        stderr: StdioCollector { id: applyErr }
        onExited: code => {
            root.busy = false;
            if (code === 0 && !applyErr.text.includes("warning")) Qt.quit();
            else root.error = applyErr.text.trim().replace(/^switchout: /gm, "");
        }
    }

    PanelWindow {
        id: win
        screen: Quickshell.screens.find(s => s.name === Hyprland.focusedMonitor?.name) ?? Quickshell.screens[0]
        anchors { top: true; bottom: true; left: true; right: true }
        exclusionMode: ExclusionMode.Ignore
        color: theme.backdrop
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
        WlrLayershell.namespace: "switchout"

        MouseArea {
            anchors.fill: parent
            onClicked: Qt.quit()
        }

        Rectangle {
            id: card
            anchors.centerIn: parent
            width: content.implicitWidth + 48
            height: content.implicitHeight + 48
            radius: 20
            color: theme.surface
            border.color: theme.surfaceVariant
            border.width: 1

            // Swallow clicks so they don't reach the backdrop.
            MouseArea { anchors.fill: parent }

            focus: true
            Keys.onPressed: event => {
                const n = root.modes.length;
                switch (event.key) {
                case Qt.Key_Escape: case Qt.Key_Q: Qt.quit(); break;
                case Qt.Key_Left: case Qt.Key_H: root.selected = (root.selected + n - 1) % n; break;
                case Qt.Key_Right: case Qt.Key_L: root.selected = (root.selected + 1) % n; break;
                case Qt.Key_Tab: root.target = (root.target + 1) % (root.externals.length + 1); break;
                case Qt.Key_Return: case Qt.Key_Enter: case Qt.Key_Space: root.apply(root.selected); break;
                default:
                    if (event.key >= Qt.Key_1 && event.key < Qt.Key_1 + n) root.apply(event.key - Qt.Key_1);
                    else return;
                }
                event.accepted = true;
            }

            ColumnLayout {
                id: content
                anchors.centerIn: parent
                spacing: 18

                Text {
                    text: "Display mode"
                    color: theme.text
                    font.pixelSize: 20
                    font.weight: Font.DemiBold
                }

                // Only worth showing when there is a choice to make.
                Row {
                    visible: root.externals.length > 1
                    spacing: 8
                    Repeater {
                        model: root.externals.length > 1 ? ["All screens"].concat(root.externals.map(s => s.name)) : []
                        Rectangle {
                            required property string modelData
                            required property int index
                            readonly property bool active: index === root.target
                            width: chipLabel.implicitWidth + 24
                            height: 30
                            radius: 15
                            color: active ? theme.accent : theme.surfaceVariant
                            Text {
                                id: chipLabel
                                anchors.centerIn: parent
                                text: parent.modelData
                                color: parent.active ? theme.accentText : theme.textDim
                                font.pixelSize: 13
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.target = parent.index
                            }
                        }
                    }
                }

                Row {
                    spacing: 14
                    Repeater {
                        model: root.modes
                        ModeCard {
                            required property var modelData
                            required property int index
                            mode: modelData.id
                            label: modelData.label
                            hint: modelData.hint
                            number: index + 1
                            selected: index === root.selected
                            current: root.status !== null && root.status.mode === modelData.id
                            enabled: !root.busy && root.status !== null && !root.status.error
                            onActivated: root.apply(index)
                            onHovered: root.selected = index
                        }
                    }
                }

                Text {
                    Layout.fillWidth: true
                    Layout.maximumWidth: 4 * 170 + 3 * 14
                    text: root.error !== "" ? root.error
                        : root.busy ? "Applying…"
                        : "←→ choose · Enter apply · 1-4 quick apply" + (root.externals.length > 1 ? " · Tab screen" : "") + " · Esc close"
                    color: root.error !== "" ? theme.error : theme.textDim
                    font.pixelSize: 12
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                }
            }
        }
    }

    component ModeCard: Rectangle {
        id: modeCard
        property string mode
        property string label
        property string hint
        property int number
        property bool selected
        property bool current
        signal activated
        signal hovered

        width: 170
        height: 170
        radius: 14
        color: selected ? theme.surfaceVariant : "transparent"
        border.color: selected ? theme.accent : theme.surfaceVariant
        border.width: selected ? 2 : 1
        opacity: enabled ? 1 : 0.4

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: modeCard.hovered()
            onClicked: modeCard.activated()
        }

        Text {
            anchors { top: parent.top; left: parent.left; margins: 10 }
            text: modeCard.number
            color: theme.outline
            font.pixelSize: 12
        }

        Rectangle {
            visible: modeCard.current
            anchors { top: parent.top; right: parent.right; margins: 8 }
            width: currentLabel.implicitWidth + 12
            height: 18
            radius: 9
            color: theme.accent
            Text {
                id: currentLabel
                anchors.centerIn: parent
                text: "current"
                color: theme.accentText
                font.pixelSize: 10
            }
        }

        Column {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: 6
            spacing: 12

            Illustration {
                anchors.horizontalCenter: parent.horizontalCenter
                mode: modeCard.mode
            }
            Column {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 2
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: modeCard.label
                    color: theme.text
                    font.pixelSize: 15
                    font.weight: Font.Medium
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: modeCard.hint
                    color: theme.textDim
                    font.pixelSize: 11
                }
            }
        }
    }

    // Laptop on the left, external screen on the right; the digit is what each shows.
    component Illustration: Row {
        property string mode
        spacing: 8
        Screen {
            width: 44; height: 30
            on: mode !== "external-only"
            glyph: "1"
        }
        Screen {
            width: 58; height: 38
            anchors.bottom: parent.bottom
            on: mode !== "internal-only"
            glyph: mode === "extend" ? "2" : "1"
        }
    }

    component Screen: Rectangle {
        property bool on
        property string glyph
        radius: 4
        color: on ? theme.accent : "transparent"
        border.color: on ? theme.accent : theme.outline
        border.width: 2
        Text {
            anchors.centerIn: parent
            text: parent.on ? parent.glyph : ""
            color: theme.accentText
            font.pixelSize: 14
            font.weight: Font.Bold
        }
    }
}
