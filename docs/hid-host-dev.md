---
title: Keyboards and mice (developer note)
layout: default
nav_exclude: true
english_only: true
---

# Keyboards and mice (developer note)

Status: prototype for issue #413. Developer mode only, off by default.
Verified on the emulator only. No real keyboard, mouse or adapter has been
tested.

## What the feature does

BlueCaster takes over the whole Bluetooth adapter, so Windows loses the
keyboards and mice connected through that adapter. With this feature on,
BlueCaster connects those keyboards and mice and passes their input to
Windows with `SendInput`. The feature supports both classic Bluetooth (HID)
and Bluetooth LE (HID over GATT) devices.

## Why the feature is dangerous

Input works only while `bluecaster-core.exe` is running and the Bluetooth
stack is up. If the PC's only keyboard and mouse are connected this way, the
user has no input when:

- BlueCaster closes, crashes or updates.
- BlueCaster has not started yet: on the sign-in screen and the lock screen.
- A UAC prompt or a window running as administrator has focus. The core
  runs without elevation, so Windows blocks the core's input there.

BlueCaster normally restarts the stack before each new audio stream,
because later streams on the same stack stuttered (issue #84). While a
keyboard or mouse is connected, BlueCaster keeps the stack running instead,
so the stutter may return. The stack still restarts after sleep, when
adapter settings change, when LE Audio is turned on or off, and when headset
mode needs voice. Each restart briefly disconnects keyboards and mice, which
then reconnect automatically.

## Turning the feature on

The `hid_host` setting takes effect only while developer mode is on. Only
the `hid.enable` request can turn the setting on:

1. The core issues a six-digit code (`hid.code`, valid for two minutes,
   usable once).
2. The dialog shows the risks, two check boxes and the code. The user types
   the code. The button stays disabled for ten seconds.
3. The core checks developer mode, both check boxes and the code again
   (`Service::hid_confirm`).

`settings.set` can turn the feature off but never on. Turning developer mode
off also saves the feature as off. While the feature is on, every screen
shows a warning banner, and BlueCaster asks before turning the feature off.

## Parts

| Part | Where |
| --- | --- |
| Report descriptor parser, decoder, key map, `SendInput` | `app/src/crates/hid-input`, `win-sys/src/input_inject.rs` |
| Shared engine: allow-list, counters, release of held keys | `bt-host/src/hid.rs` |
| Classic HID host (BTstack `hid_host`) | `bt-host/src/hid.rs`, `btstack-sys/csrc/shim.c` |
| LE host (HID over GATT) as an LE module | `bt-host/src/le/modules/hogp.rs` |
| Pairing with a number shown | `bt-host/src/hid/pairing.rs`, `btstack-sys/csrc/shim.c` |
| Keyboard lights | `hid-input/src/leds.rs`, `bt-host/src/hid.rs`, `hogp.rs` |
| Guard against held keys | `win-sys/src/hid_guard.rs`, `crates/core/src/hid.rs` |
| Where input goes (this process, the input helper, or counted only) | `hid-input/src/sink.rs`, `crates/input-helper`, `win-sys/src/input_desktop.rs` |
| Setting, device list, confirmation, keeping the stack up | `crates/core/src/hid.rs` |
| IPC (`hid.*`), settings rules | `crates/ipc/src/protocol.rs`, `core/src/state.rs` |
| UI | `ui/src/components/HidHost.tsx`, `HidEnableDialog.tsx` |

The code follows these rules:

- Only devices on the device list may connect. BlueCaster refuses any other
  device.
- However a connection ends, the code releases any keys and buttons still
  held.
- Logs contain counts only: no addresses, key codes, typed text or pairing
  numbers.
- Only the device being added pairs with a number shown, for at most 90
  seconds or until its pairing ends. Every other device pairs as before.
- With the feature off, none of its code runs: no HID channels, no LE for
  keyboards and mice, no extra scan entries.

With the feature on and LE Audio off, the stack starts LE for keyboards and
mice only. No LE Audio service, scan entry or UI state appears.

## Pairing with a number

Some keyboards ask for a number when they are added. BlueCaster shows six
digits, and the user types the digits on the keyboard and presses Enter.
Older keyboards use a PIN instead, and BlueCaster provides the PIN the same
way. The PIN path is untested. The CLI prints the number only over
`--hci-tcp`, so use the app to add a real keyboard that asks for a number.

## LE devices that change their address

A paired LE device that uses a private address reconnects automatically.
BlueCaster scans in the background and recognises the device by the pairing
keys. With LE Audio off, the adapter also resolves the address where
possible. After pairing, the device list stores the device's identity
address.

## Keyboard lights

BlueCaster sends the Num Lock, Caps Lock and Scroll Lock states to every
connected keyboard: once after the keyboard connects, and again whenever a
state changes in Windows. BlueCaster checks every 150 ms.

## Keys held when BlueCaster dies

A hidden guard (`bluecaster-core.exe --hid-guard`) releases the keys and
buttons still held if the core exits or crashes. The core starts the guard
while passing input to Windows and records the held keys and buttons in
shared memory. When the feature is turned off, the guard exits without
releasing anything.

## Files

- `devices/hid_devices.txt`: `address|name`, with a third field `le:public`
  or `le:random` for LE devices.
- `devices/hid_descriptors/`: each device's report descriptor and, for LE
  devices, its report handles, so that input can be decoded as soon as the
  device reconnects.

## Testing

```
pwsh ./build.ps1 -Decoder
python tools/emu/run_test.py --hid
```

The emulator test never types into the PC: over `--hci-tcp`, BlueCaster
only decodes and counts input. The 18 cases cover connections in both
directions, a device that is not on the device list, a link lost with a key
held, the saved descriptor, LE pairing, LE reconnection without pairing
again, an LE device whose address changes, input sent right after an LE
reconnection, pairing with a number (classic and LE), keyboard lights
(classic and LE), the LE scan, and the feature turned off.

`cargo test -p bluecaster-core-exe --test hid_guard` tests the guard without
passing any input to Windows.

To test by hand, run
`bluecaster-core --cli --hid-test 60 --hid-device <address>`. The command
prints the counts once a second. Add `--hid-inject` to pass the input to
Windows.

## Known limitations

- Address resolution by the adapter is untested. The emulator does not
  resolve addresses, so the tests covered only the background scan. With LE
  Audio on, BlueCaster uses only the background scan.
- Reading the lock keys from Windows, passing real input to Windows and a
  real release by the guard are untested, because the emulator test never
  touches the PC's input.
- Outside the service, input does not reach the sign-in screen, the lock
  screen, UAC prompts or administrator windows. The input helper in the
  service (stage 4) has not been tested on these screens yet.
- Games with anti-cheat may ignore injected input.
- BlueCaster does not pass input from gamepads or absolute pointing devices
  to Windows.
- If an LE device has changed its report handles since the last connection,
  BlueCaster may drop input that arrives before the device's services are
  read again.

## Hardware checklist (not done yet)

- [ ] Classic mouse and keyboard: add, type, reconnect after power off.
- [ ] LE mouse and keyboard: add, type, reconnect after sleep.
- [ ] Audio stream running while a mouse moves: dropouts, pointer lag.
- [ ] Codec switch and headphone reconnect: how long the mouse stays disconnected.
- [ ] Sleep and resume.
- [ ] BlueCaster killed with a key or button held: the guard releases the
      key or button.
- [ ] A keyboard that asks for a number (classic and LE): the right
      number pairs the keyboard, a wrong number fails cleanly.
- [ ] An LE device with a changing address: reconnects after power off.
- [ ] Lock key lights on classic and LE keyboards, also after a reconnect.
- [ ] Several streams in a row with a mouse connected: does the stutter of
      issue #84 return?
- [ ] An adapter without Bluetooth LE.

## Moving to a Windows service (in progress)

Goal: keyboards and mice also work on the sign-in screen, the lock screen,
UAC prompts and administrator windows. No process run by the signed-in user
can tamper with the process that produces input. To get there, the whole
core moves into a service. An agent in the user's session does only the work
that needs that session.

Stage 1 is done. With `--capture-agent`, a second process captures the audio
and passes it to the core through shared memory (`crates/pcm-link`,
`audio-capture/src/remote.rs`, `win-sys/src/shm.rs`). This second process is
the agent (`bluecaster-core.exe --agent`).

The foundation of stage 2 is done too: `crates/session-host`. With
`--session-agent`, the core serves the agent pipe
`\\.\pipe\bluecaster-agent-<user SID><instance>`. `--capture-agent` still
works. The core starts one long-lived agent (`--agent --child`) whenever no
agent is connected. An agent started by hand (`--agent`) waits for a core
and serves that core instead. When that core exits, the agent waits for
another core.

Requests carry ids in both directions. The agent sends events, including
the agent's log lines. PCM goes through channels in both directions
(`host/capture.rs`, `host/render.rs`).

When the agent dies, waiting requests fail with "agent lost". Running
captures end with `EndReason::AgentLost` and are reopened. The next agent
serves new requests.

The rest of the core calls the facade (`session_host::host()`, trait
`SessionHost`) instead of calling Windows. To add a group of operations,
see the doc comment in `session-host/src/lib.rs`. Stage 3 checks the
connecting agent in one place: `authorize` (hub.rs). Stage 4 starts the
agent in the user's session in one place: `launch_agent` (hub.rs).

Stage 2 is done. Every group of user-session work goes through the facade:

| Group | What runs in the agent |
| --- | --- |
| `devices`, `default_device`, `endpoints` | Device lists and formats, switching and restoring the default output and input, device change notifications, the app list |
| `volume`, `activity` | Output volume and mute, the Volume Mixer, watching an app's process, microphone-in-use checks |
| `capture`, `render` | Audio capture, and the headset microphone output |
| `media` | The Windows media session and media keys |
| `shell`, `autostart`, `elevation` | The tray, notifications, opening the UI, the Run entry, the driver helper and the VB-CABLE setup |

`bluecaster-core.exe --headless` runs the core without a tray or window,
with the agent turned on. The service will host this mode. While a headless
core is connected, the agent shows the tray.

The agent holds some changes on the core's behalf: a switched default
device and a muted output. If the core dies, the agent restores the default
device and unmutes the output. If the agent dies, the core reapplies both
changes through the next agent. Nothing on the BTstack thread or an audio
thread waits for the agent.

Run the tests with `cargo test -p session-host` and
`cargo test -p bluecaster-core-exe --test session_agent`. The second command
runs the real agent program. Tests that would change the PC (default
devices, volume, mute, the Run entry, elevation, the tray) use fakes.
Emulator cases that use the agent: `sbc-agent`, `aac-agent`,
`sbcxq-44k-agent`, `sbc-agent-killed`, `sbc-agent-first`,
`sbc-agent-replaced`, `avrcp-keys-agent`, `msbc-agent` and
`cvsd-agent-killed`. For LE Audio: `stereo-agent`, `mic-stereo-agent`,
`mcp-read-agent` and `mcp-control-agent`.

Not verified: the real tray in the agent, real elevation through the agent,
real default device, volume and mute changes through the agent, and the
headset microphone played into a real output. The core and the agent still
run as the same user. The installed app does not use the agent yet.

### Service data (stage 3)

`--headless --service-data <folder>` runs the core in service mode. The
core keeps all its data in that folder instead of the user's folder. The
service will pass `%ProgramData%\BlueCaster`. Only SYSTEM, the
Administrators and the service's own account can open the folder
(`win-sys/src/acl.rs`). In service mode, the core encrypts the pairing keys
for its own account (`dpapi::Scope::Service`). The window keeps its log in
the user's folder. `crates/core/src/service_data.rs` lists where each file
lives.

At its first start, the core moves the user's data into the folder once,
through the agent (group `migration`). The agent reads a fixed list of
files and decrypts the keys. The core checks each file and encrypts the
keys again. The user's files stay where they are. The move never takes the
keyboard and mouse list, and it turns the feature off. Driver records,
Windows' replaced keys and firmware stay behind too. Run
`cargo test -p bluecaster-core-exe --test service_data` to test the move
with a real agent.

### Pipes and request rules (stage 3)

In service mode, the core serves the UI on `\\.\pipe\bluecaster-service`.
Interactive users can connect, but they cannot create another instance of
the pipe. The core reads each client's identity from Windows
(`win-sys/src/pipe_peer.rs`): process, session, user, elevation and program
path. Before trusting the pipe, the UI checks the core's identity the same
way. The agent must run in the console session and must be the program
file next to the core's.

Every request has a class (`ipc/src/access.rs`, `core/src/rules.rs`).
Adding a request without a class breaks the build.

| Class | Requests |
| --- | --- |
| Open | Streaming, scanning, profiles, ordinary settings, the Volume Mixer, turning this feature off, the code for turning it on (`hid.code`), Exit (`app.close`) |
| Needs an administrator | Turning this feature on, adding or removing a keyboard or mouse, developer mode and the settings behind it, raw adapter commands, driver changes, VB-CABLE setup, reading pairing keys, captures and bundles |
| Refused | Quitting the core, and any file path from the client |

When a request needs an administrator, the UI sends `app.run_as_admin`. The
core issues a single-use ticket for that exact request. The agent starts
`bluecaster-core.exe --request` elevated, so Windows shows its own prompt.
The core accepts the ticket only from an elevated copy of BlueCaster.

### Logs and test switches (stage 3)

The agent and the core both redact every forwarded log line
(`session-host/src/redact.rs`). Placeholders replace quoted names, paths,
file names, IDs and addresses. Log statements that can run in the agent
must put every name in quotes. The test switches (`--test-shell`,
`BLUECASTER_TEST_FAKE_VOLUME`, the guard's `--report`) work only in debug
builds. A release build given a test switch exits with code 2.

Not verified in stage 3: a real elevated request and the Windows prompt, a
core running as SYSTEM or as a service account, and the real
`%ProgramData%` folder. The tests simulate an elevated client, which only
the test build allows.

### The service and the user's session (stage 4)

`bluecaster-core.exe --service` is the service's entry point. Windows'
service control manager starts it. The service runs the headless core in
service mode, with the data folder `%ProgramData%\BlueCaster`
(`core/src/service_host.rs`, `win-sys/src/service.rs`). It reports start
and stop progress, stops the Bluetooth stack on stop and shutdown, and
passes session changes to the agent's hub. Sleep and resume use the
core's own power notifications, which work in a service too.

`--install-service` and `--uninstall-service` create and remove the
service and need an administrator. The service starts automatically,
without a delay, so that keyboards and mice work on the sign-in screen.
Windows restarts it after a crash. It runs as LocalSystem and keeps only
four privileges. `service_host.rs` explains why, and what a split into a
low-privilege stack host and a small launcher would take.

The agent starts in the active console session once a user signs in
there (`session-host/src/sessions.rs`), with that user's token and a clean
environment (`win-sys/src/session_launch.rs`). Before anyone signs in
there is no agent, and the core runs without one. When another user takes
the console (fast user switching), a running stream stops and the old
user's agent ends. The stream's audio belongs to the old user.

In service mode the agent connects to `\\.\pipe\bluecaster-service-agent`,
and checks that the pipe's server is the core's own program. The core
accepts only the process it started, in the session it started it in.
PCM channels go to the agent as handles duplicated into its process,
because a service and the user's session share no `Local\` names.

The data move runs when the first agent connects. The user's files
replace the defaults the service wrote before anyone signed in
(`migration_pending.json`), and the core reads the moved data again
without a restart.

Run the tests with `cargo test -p bluecaster-core-exe --test
service_session`. A test file stands for the console's session
(`BLUECASTER_TEST_SESSIONS`, debug builds only). Never tested: the real
service, the agent started with another token, and the install commands.

### What the user gets in service mode (stage 4)

The signed-in user cannot open the service's folder, so in service mode:

- **Files the user asked for.** Diagnostic bundles, chip probe reports
  and chip script reports reach the user through the agent (group
  `handover`, `core/src/handover.rs`). The core sends only the file's kind
  and content. The agent picks the folder and file name
  (`logs\bundles\bluecaster-diag-<time>.zip` and
  `logs\probe\probe-<time>.json` in the user's BlueCaster folder), It checks
  the size and content, then returns the path. The core never reads
  from the user's folder. All three requests need an administrator, so
  an administrator approved every file the user receives. The agent writes to the console user's profile, not to the
  profile of the administrator who approved the prompt.
- **HCI captures** stay in the service's folder, because stopping a
  capture needs no administrator. A capture reaches the user inside a
  bundle. The UI shows only the capture's file name.
- **Folders.** The snapshot sets `service_mode` and leaves `config_dir`
  and `log_dir` empty. The UI hides the buttons that open the settings,
  firmware and log folders. The firmware download still works.
- **Logs.** The service's log contains no app names, device IDs or paths
  (`bluecaster_core::diag::log_name`). The Diagnostics screen still shows
  the log lines, and bundles include the log files.
- **Verbose logging.** The UI cannot read the core's `verbose.json` and
  `settings.json`. While the UI is connected to the service, its log
  level follows the snapshot's `verbose` and developer mode's
  `dev_log.always_verbose` (`ui/src-tauri/src/logging.rs`).
- **Exit.** `app.quit` is refused. The tray's Exit, the Quit button and
  closing the window without "Minimize to System Tray" send `app.close`.
  Streaming stops, the UI closes (event `ui_close`) and the tray icon
  disappears.
  The core keeps running, so keyboards and mice keep working. The tray
  icon returns when BlueCaster's window reconnects or a new agent connects.
- **Updates.** In-app updates are off. The UI shows the release page instead.
- **Turning on keyboards and mice** asks for an administrator once.
  `hid.code` is open, because the code alone turns nothing on. `hid.enable` needs
  an administrator, so the prompt appears only after the user ticks both
  check boxes, types the code and waits ten seconds. The core still checks
  developer mode, both check boxes and the code.
- **Is the pipe's server the service?** The UI and the elevated helper
  ask the service control manager (read-only, `win_sys::service_query`)
  for the service's program and process. The pipe server must be that process,
  run that program and run in session 0.

### The input helper (stage 4)

A service cannot pass input to the user's desktop or read the lock keys.
So a headless core hands keyboard and mouse input to a helper,
`bluecaster-core.exe --input-helper <pipe>`, in the console session (crate
`input-helper`). Under the service, the helper runs as SYSTEM, which can
switch to the sign-in screen, the lock screen and UAC prompts. When you
start the core by hand, the helper is a plain child process of the core. The resident app and
the command line still pass input themselves, with the guard.

- **The pipe.** For each helper, the core creates a pipe with a random
  name that only SYSTEM and the core's user may open. The core then
  starts the helper. The helper accepts the pipe only when the pipe server
  is its own parent process, and keeps a handle to that parent open. The
  core accepts only the client process it started. Frames are small and binary: key, button,
  move and wheel records with checked fields and bounded counts, and no
  paths or text. Any malformed frame stops the helper.
- **The queue.** The BTstack thread only adds input to a queue. When the
  queue is full, the core drops the oldest pointer moves (counted as
  `dropped`). Key and button transitions are never dropped singly. If the
  queue fills with transitions alone, the core discards all of them and
  the helper releases every key and button instead.
- **Held keys.** The helper releases every held key and button when the
  core closes the pipe or exits, when asked to stop, on a malformed frame,
  and on the new desktop after a desktop switch. The core tracks what the
  helper may be holding. If a helper is killed before it can release
  anything, the next helper releases those keys and buttons first.
- **Lock keys.** The helper reads the lock keys on its desktop every
  150 ms and reports changes. The core no longer reads them.
- **Another user at the console.** The helper releases all keys and
  stops, and a new helper starts in the new session. When no one is at the
  console, the core drops input.
- **Ctrl+Alt+Del.** `SendInput` cannot produce it. The helper reports it,
  and the service calls `SendSAS` only if the policy "Disable or enable
  software Secure Attention Sequence" (`SoftwareSASGeneration` under
  `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System`) allows
  services (1 or 3). BlueCaster never changes the policy. Windows leaves
  it off because allowing it lets every service on the PC open the secure
  screen.

Run the tests with `cargo test -p input-helper` and `cargo test -p
bluecaster-core-exe --test input_helper`. In debug builds, the helper's
`--record` option writes the input to a pipe that the test reads, instead
of calling `SendInput`. The emulator cases
`hid-incoming-helper`, `hid-release-helper` and `hid-le-release-helper` run
the CLI's `--hid-helper-record` and need a debug build. Not tested yet: the
helper's real `SendInput`, desktop switches on a real secure desktop, the
helper running as SYSTEM, and `SendSAS`.

Still to do:

1. Test the service by hand (next section). None of the service code has
   run as a Windows service yet.
2. Test the input helper as SYSTEM on real hardware: the sign-in screen,
   the lock screen, a UAC prompt and fast user switching.
3. Change the installer and updater to install for all users under Program
   Files. Today, BlueCaster is installed per user, so another program running
   as the same user can replace BlueCaster's files and pass the program
   checks. Tell authors of other pipe clients how to open the service pipe.
4. Decide what Windows users on the same PC share. Today, the core
   migrates only the first user's data, and every user sees the same devices.
5. Decide whether to split the service into a low-privilege process for the
   Bluetooth stack and a small LocalSystem launcher. Today, one LocalSystem
   service does both, so the stack handles radio data as SYSTEM.
6. Decide whether headphones may reconnect after Exit while the tray icon
   is hidden.

### Testing the service by hand

None of these steps have been run yet. Use a test PC or a virtual machine with a spare
adapter, and keep a USB keyboard and mouse connected. The per-user install
makes the program checks weak, so do not leave the service installed on a
PC you rely on.

1. Exit the installed copy of BlueCaster.
2. From an elevated prompt, run `bluecaster-core.exe --install-service`.
   Check the result with `sc qc BlueCaster` and `sc qfailure BlueCaster`.
3. Run `sc start BlueCaster`. The log is in
   `%ProgramData%\BlueCaster\logs`. The tray icon appears when the agent
   starts in your session.
4. Start BlueCaster. The window must connect to the service. Turn on
   keyboards and mice. Windows asks for administrator approval once, after
   the code and the wait.
5. Add a Bluetooth keyboard or mouse. Then check input in each place: on
   the desktop, in an administrator window, at a UAC prompt, on the lock
   screen, on the sign-in screen after a restart, and after fast user
   switching.
6. Hold down a key and end `bluecaster-core.exe --input-helper` in Task Manager.
   The key must be released when the next helper starts.
7. Run `sc stop BlueCaster`. The service must stop within a few seconds.

To remove the service, run `bluecaster-core.exe --uninstall-service` from an
elevated prompt, or `sc delete BlueCaster`. To remove the service's data as
well, delete `%ProgramData%\BlueCaster`. The user's own folder is not
affected.

Ctrl+Alt+Del from a Bluetooth keyboard needs a Windows policy that lets
services raise the secure screen. BlueCaster never changes that policy. The
policy's registry value name and numbers in `win-sys/src/input_desktop.rs`
were written from memory. Check them against Microsoft's documentation
before relying on them.
