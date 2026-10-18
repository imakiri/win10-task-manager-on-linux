/* Copyright 2026 Mission Center Developers
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

// Panel button for `taskmanager-launcher`. As a launcher menu, Plasma gives it the Meta key
// ("Activate Application Launcher"). Both Meta and a click call Toggle over D-Bus; D-Bus starts
// the launcher (registered with `taskmanager-launcher --install-activation`) when none runs.

import QtQuick

import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.plasmoid

PlasmoidItem {
    id: root

    // The button itself, no popup
    preferredRepresentation: fullRepresentation
    Plasmoid.icon: "io.github.taskmanager.TaskManager"

    Plasmoid.onActivated: toggle()

    function toggle() {
        runner.connectSource("dbus-send --session --type=method_call "
            + "--dest=io.github.taskmanager.TaskManager.Launcher /Launcher "
            + "io.github.taskmanager.TaskManager.Launcher.Toggle")
    }

    Plasma5Support.DataSource {
        id: runner
        engine: "executable"
        connectedSources: []
        onNewData: sourceName => disconnectSource(sourceName)
    }

    fullRepresentation: Kirigami.Icon {
        implicitWidth: Kirigami.Units.iconSizes.medium
        implicitHeight: Kirigami.Units.iconSizes.medium
        source: Plasmoid.icon
        // Fallback while the application icon is not installed
        fallback: "start-here-kde-plasma"
        active: mouse.containsMouse

        MouseArea {
            id: mouse
            anchors.fill: parent
            hoverEnabled: true
            onClicked: root.toggle()
        }
    }
}
