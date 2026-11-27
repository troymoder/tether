# Features

## Adding devices

- **By cable to tetherd's machine.** Android phones and iPhones show up by themselves.
- **By cable to the browser's machine.** Android only, through WebUSB, so Chromium browsers only.
- **Over Wi-Fi.** Android only: by address, a pairing code, or a QR code the phone scans.
- **As an emulator.** tetherd creates Android emulators and can start them when it starts.

A new browser needs a setup code from tetherd's log, or approval from a browser that is already
trusted.

## Views

The device's screen sits beside every view, with touch, keyboard, sound, screenshots and recording.

| View        | What it does                                                           |
| ----------- | ---------------------------------------------------------------------- |
| Devices     | Lists, adds, starts, renames and removes devices.                      |
| Registry    | Holds uploaded app builds and installs them on devices.                |
| Apps        | Opens, stops, updates, clears and uninstalls a device's apps.          |
| Logs        | Follows logcat with filters, and an emulator's own output.             |
| Crashes     | Lists recorded crashes.                                                |
| Shell       | A terminal on the device.                                              |
| Files       | Browses, searches, uploads, downloads and edits files.                 |
| Database    | Reads an app's SQLite databases and preferences files, and runs SQL.   |
| Media       | The device's photos and videos.                                        |
| Clipboard   | Moves text between the device and the computer.                        |
| Camera      | Feeds an emulator's camera, or mirrors a phone's own.                  |
| Location    | Sets the device's location on a map and plays back GPX and KML routes. |
| Preferences | Device settings such as dark theme, text size and language.            |
| Access      | Trusted browsers, and browsers waiting for approval.                   |
| Network     | The connection between this browser and tetherd.                       |
| Events      | A log of what tetherd and its browsers did.                            |
| Settings    | The page's own theme, cache and allowed USB phones.                    |

## By kind of device

| Feature                               | Android phone            | Emulator | iPhone                       |
| ------------------------------------- | ------------------------ | -------- | ---------------------------- |
| Screen, sound, screenshots, recording | yes                      | yes      | yes                          |
| Input                                 | yes                      | yes      | one finger, buttons and keys |
| Apps, Logs, Crashes, Shell            | yes                      | yes      | no                           |
| Files, Database                       | no app data without root | yes      | no                           |
| Media, Clipboard, Location            | yes                      | yes      | no                           |
| Camera                                | mirrors its own cameras  | fed      | no                           |
| Preferences                           | no language without root | yes      | no                           |

## Not built yet

- Linking more than one tetherd. The page has the controls, and they only say so.
- iOS apps, iPhone simulators and iPhones over WebUSB.
- Builds other than APKs in the registry.
