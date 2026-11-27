# Architecture

tether is two programs. **tetherd** runs on the machine the devices are attached to. **The page**
runs in a browser and is served by tetherd. There is one user and no accounts.

## Parts

| Crate                   | Role                                                                   |
| ----------------------- | ---------------------------------------------------------------------- |
| `crates/server`         | tetherd: serves the page, holds devices, runs emulators, stores state. |
| `crates/web`            | The page, compiled to WebAssembly.                                     |
| `crates/protocol`       | The messages tetherd and the page exchange.                            |
| `crates/adb`            | The adb protocol over any open connection.                             |
| `crates/android`        | Android device features, written against `crates/adb`.                 |
| `crates/android/helper` | The helper, a small program run on the device.                         |
| `crates/ios`            | Pairing with an iPhone and sending it input.                           |
| `crates/quicktime`      | An iPhone's screen and sound, over USB.                                |
| `crates/vcam`           | The shim, which gives an emulator its cameras.                         |
| `crates/vcam-wire`      | The messages between the shim and tetherd.                             |

## The page and tetherd

The page holds one WebRTC connection to tetherd, set up with two HTTP requests. WebRTC was chosen
over a WebSocket because a device's screen needs real video and audio tracks, and the same
connection carries everything else:

- A control channel carries requests, replies and state.
- Each bulk job, such as a file transfer, gets a channel of its own, so the control channel never
  waits behind it. The code calls these lanes.
- Screen video and sound go on tracks.

State is pushed. The page subscribes to topics (devices, apps, access and so on) and tetherd sends
the items that changed.

## Trust

Each browser has a key, and tetherd knows a browser by its fingerprint. The first browser proves it
knows a setup code that tetherd prints in its log. Later browsers ask, and a trusted one approves
them. The setup code itself is never sent, only a proof of it, because the request may travel over
plain HTTP. tetherd accepts HTTP from its own machine only.

## Devices

**Android.** `crates/adb` speaks the adb protocol itself, with no adb server or program. The same
code therefore runs in tetherd and in the page, over USB, TCP or a relayed channel. Most device
features run in the page. They reach a phone either through tetherd, which relays adb channels
unchanged, or through the browser's own WebUSB when the phone is plugged into the browser's machine.
For data adb cannot give cheaply, such as app labels and photo thumbnails, the page pushes the
helper to the device.

**Emulators.** tetherd creates and runs the Android SDK's emulators without a window.

**iPhones.** An iPhone connects to tetherd's USB. Its screen comes over the protocol QuickTime uses
to record a phone, because Apple's newer display stream turns the phone's camera off. The pairing
record never leaves tetherd.

**Screens.** tetherd captures a device's screen once and shares it with every page watching.

**Emulator cameras.** The shim is loaded into the emulator and stands in for a camera device, so no
kernel module is needed. It can be fed from the browser (a camera, a shared screen, a file) or from
a webcam or file on tetherd's machine.

## Inside the page

Each view declares what it needs from a device as a trait (`views/<view>/device.rs`), and each kind
of device implements the traits it supports
([crates/web/src/device.rs](../crates/web/src/device.rs)). A view then works for any device that
implements its trait, and a new kind of device is a new implementation.

## Storage

tetherd keeps one SQLite database and its keys in its data folder. The page keeps its key and
preferences in the browser.
