# Using Atlas

Use Atlas to open an app in the helper, inspect its screen and test what happens after an action. The helper must be logged in and running in the background while you stay in your own account. Finish [setup](getting-started.md) first.

## Test an app

Build your app normally, outside Atlas. In this example, replace `/path/to/build/Example.app` with your app bundle and `Example` with its running app name.

```sh
atlas doctor --json
```

Proceed only when `ready` is true. Then open the build and wait for a window:

```sh
atlas open /path/to/build/Example.app
atlas wait --app Example
atlas screenshot
```

Atlas copies the app for the helper when the helper cannot read the original path. The screenshot result gives you a PNG path. Open it before choosing an action. `atlas apps` lists the running app names if the name differs from the bundle's filename.

Inspect the window's controls:

```sh
atlas windows --app Example
atlas elements --app Example --role AXButton
```

Each element has its role, name, click point (`at`) and size. The click point is where a named click lands: the middle of the element, or the box of a checkbox or radio button. Choose a visible control from that result. If your app has a button named **Settings**, for example:

```sh
atlas click --app Example --name Settings --role AXButton
atlas screenshot
```

The named click refuses missing or ambiguous targets. Each listing also names its `look`, such as `8123abc`. `atlas click --app Example --ref 8123abc.12` clicks the element with `index` 12 in that listing: that very element where it is now, with the same checks as a named click. Atlas never matches a reference again by its name or position: when the listing has expired, the app or Atlas's input service restarted, or the element was replaced, changed or moved, the click is refused with `performed: 0` and tells you to run `atlas elements` again. Inspect the second screenshot to check the result. A successful input response confirms delivery, not that the app did what you intended. Check the resulting screen, accessibility elements or saved file after each meaningful step.

An element of a listing can also be acted on through Accessibility, with no pointer, keys or focus:

```sh
atlas element press --ref 8123abc.12
atlas element set-value --ref 8123abc.4 --value "Quarterly report"
atlas element check --ref 8123abc.7
```

The actions are `press`, `set-value --value TEXT`, `check`, `uncheck`, `pick --option LOOK.INDEX` (an option listed under that control), `expand`, `collapse` and `scroll-into-view`, and `atlas input` takes them as actions of those types. The element is found again and proven the same as a referenced click finds it, its window must not be blocked by a sheet or modal dialog, and it is proven once more just before the one request that acts. `check`, `uncheck`, `expand` and `collapse` ask for a state: an element already in it is left alone, with `performed: 0` and `verification: matched`, and one whose state cannot be read is refused rather than toggled. The reply is `{"ok":true,"performed":1,"mechanism":"accessibility","verification":"matched"}`, where `verification` is `matched` or `mismatch` when the element's state or value was read back, and `not-checked` otherwise, as after a press. It never repeats the value you set. These are for an app's own controls: an element of a web page (one inside its `AXWebArea`) is refused with nothing sent, and there you click and type instead. An element that does not support the action is refused with nothing sent; Atlas never falls back to a click. A password field's value is never set. Apps can respond to Accessibility differently than to a real click or typing, and the app may still move focus itself. An action the app did not acknowledge in time is uncertain: do not repeat it, recover as for any uncertain input.

If a control has no usable accessibility element, take a fresh screenshot and use `atlas click X Y` with the observed position. Coordinates are display points measured from the top-left corner. Convert screenshot pixel coordinates with the result's `origin` and `pointsPerPixel`: point = origin + pixel × pointsPerPixel. `atlas screenshot --app NAME` or `--region X,Y,W,H` takes a smaller picture of one app's windows or one part of the display. Do not reuse coordinates after the window or display changes.

When you finish, close this app:

```sh
atlas close --app Example
```

This quits the app and removes any copy Atlas staged for it. Close the previous build before opening a replacement. Then undo what the task left in the helper, such as copies into it and apps installed for it:

```sh
atlas cleanup          # see what it would do
atlas cleanup --apply
```

Cleanup quits only what runs from the task's own copies and installs, never an app opened by name, moves installed apps aside in the helper, where they are kept for 7 days before they are removed, removes its copies, and reports what it kept and why. Use the [command reference](../agents/atlas/skills/atlas/references/commands.md) for typing, key combinations, scrolling and other input options.

## Watch the helper

Click the Atlas menu bar item for a small screen and current status. Choose **Open Atlas**, open Atlas from Applications, or run `atlas monitor`, to get the full native viewer. It opens on your desktop while the apps you are testing stay in the helper. If it was open when Atlas last stopped, as during `atlas update`, it opens again when Atlas starts; closing it, or choosing **Quit Atlas** (⌘Q), keeps it closed.

The viewer has two places:

| Place    | What you can see                                                                            |
| -------- | ------------------------------------------------------------------------------------------- |
| Screen   | The helper's current picture.                                                               |
| Activity | Each session's work, run by run in each app, with its problems and the screenshots it took. |

The viewer opens on Screen. Click a session in the sidebar to see its Activity, or **All sessions** to see every session's. **View > Screen** (⌘1) and **View > Activity** (⌘2) switch between them too.

Activity lists sessions, then the runs of work each did in one app, then each run's steps. A folded run shows its problems and its newest screenshots; click **N pictures** to see all of them in place, or open the run to read its steps in order. A step with a picture of the helper just before it has a small picture button; it opens the picture in a sheet, marked where the agent clicked, typed or scrolled, with **Before** and **After** when there is a picture after it. Use the arrows to step through the run's pictures, and **Replay** on an open run to play its steps back. Pictures stay on your Mac for 7 days unless **Settings > Activity** says otherwise, where you can also turn them off, limit the typed text kept beside them, or delete them all. Text typed into a password field is never kept, and no agent is ever given a picture. The [configuration guide](configuration.md#settings-fields) has the details.

In the sidebar, each session is named by what it did and where, such as "Typing in TextEdit". Sessions that never acted in the helper are not listed. While Screen shows, no session is highlighted; the session Activity shows is.

When you leave Screen, a small screen remains over Activity. Drag or throw it toward another corner to move it out of the way. Click it to return to Screen. It follows the viewer's picture setting, including a paused picture.

You can choose **Live**, **Every 5 Seconds** or **Paused** for the picture. Pausing the picture does not stop work in the helper. The menu bar panel's pause control affects its own picture. In the full viewer, **Actual Size** shows one display point per view point and lets you scroll when the screen does not fit.

Some agent hosts can show Atlas inside the conversation. That is a different viewing route from the native window; see [Agent connections](agents.md) for host support.

To watch inside a Rex terminal pane, use `atlas monitor --terminal`. The [terminal viewer guide](terminal-viewer.md) covers requirements, splitting a pane and keyboard controls.

## Take control or stop agents

Choose **Take Control** in the viewer to use your mouse and keyboard in the helper. Atlas waits for control to become available before accepting your input. While you hold control, agents cannot send competing input. Choose **Give Back**, or press **Control-Shift-Esc**, when you are done.

In the native viewer and a visible embedded view, control ends when no displayed picture has been confirmed for 6.5 seconds. A confirmed unchanged picture counts as fresh; successful control renewals alone do not. This prevents a frozen picture from holding control indefinitely.

**Stop** cancels agent input underway and blocks further agent actions. It is separate from pausing the picture and from taking control. Use **Resume** when you want agents to act again. An agent must not resume itself after you stop it. The terminal viewer (`atlas monitor --terminal`) is your tool too: like Stop and Resume, it holds against agents using Atlas's commands, not against a shell that runs it for them.

If Atlas reports that an action's outcome is unknown, do not send it again. Observe the helper if it is available, follow Atlas's recovery instructions, then inspect the result before continuing. If the helper is unavailable, complete the login or permission step first. Recovery does not prove that the earlier action did nothing. The [error reference](../agents/atlas/skills/atlas/references/troubleshooting.md) gives the next step for each error code.

## What the helper shares

The helper is a second account on your Mac, not a virtual machine.

| The helper has its own                                  | It shares with you                                                            |
| ------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Pointer, keyboard focus and screen                      | Speakers: its sound plays on yours unless Atlas mutes it                      |
| Clipboard                                               | Network, including `localhost` servers on both sides                          |
| Files, settings and app data in its home                | `/Users/Shared`, external drives and folders in your home that aren't private |
| Keychain and sign-ins: none, unless someone signs it in | CPU, memory and graphics                                                      |
| Running copies of apps                                  | The process list                                                              |

Agents' clicks and keys go only to the helper's own session, so your pointer and typing are never moved or interrupted.

### Memory and CPU

A logged-in helper runs macOS's own services for a second account, so it uses memory even with nothing open. How much varies with the apps open, how long it has been logged in and whether you watch it live. Atlas's own processes in the helper use about 30 MB. Each app adds what it would on your desktop; Chrome with one page and TextEdit added about 0.5 GB on our test Mac.

Every picture of the helper's screen comes from one screen stream in the helper: the live view, screenshots, and the pictures before and after actions. macOS's screen recorder there keeps about 1 KB of every single picture taken another way, until it restarts, but nothing measurable of the stream. A picture opens the stream when it isn't open, which takes up to a second once, and the stream stays open for 10 minutes after the last picture, or while the live view is watched. A screenshot of an app or a region is cut from the same stream. Once the recorder passes 1 GB anyway, or 300 MB the first time Atlas looks at it after an update (one grown under an earlier version), Atlas restarts it between two pictures, when no clicks or keys have come for 2 seconds; any that come meanwhile wait a moment and are never refused. The live view pauses for a moment and carries on, and Screen Recording stays allowed. If Atlas can't restart it, `atlas doctor` says so: log the helper out and back in to reset it.

With no viewer showing it, an idle helper uses little CPU. Watching costs more: a live picture of a 2560 × 1440 helper uses most of one core, split between the helper and Atlas on your side. Choose **Every 5 Seconds** or **Paused**, or close the viewer, when you don't need it; work in the helper carries on.

### Sound

Atlas keeps apps in the helper silent on your speakers, and your own sound is never touched. Muting needs one permission in the helper: the first time you switch to it, macOS asks to let Atlas record the helper's audio. Allow it there; Atlas never reads that audio, it only stops it reaching your speakers. Nothing is muted until you allow it, and `atlas doctor` says so.

Turn the helper's sound on with `atlas sound on` or in Atlas › Settings › General, and off again with `atlas sound off`; `atlas sound` says which it is. The helper keeps this choice itself. After the helper's Atlas restarts, its apps can be heard for a moment before the mute is back. Muting your Mac still mutes both accounts.

### Apps you also use

Apps that allow one copy, such as Slack, Obsidian or a password manager, allow one copy per account. When an agent opens one in the helper, it starts a second, separate copy with its own data. It is signed out, it can't see your copy, and your copy keeps running.

A few apps run in only one account at a time, such as Tailscale. While yours is open, the helper can't open it, and Atlas says so. While the helper's is open, macOS won't open yours; the agent closes it with `atlas close`.

A password manager in the helper starts with an empty vault, can't use Touch ID, and its browser extension only talks to the helper's copy. Atlas never signs it in. Keep it that way: an agent can use whatever the helper is signed in to.

### Your files

The helper starts with none of your files. To give an app in the helper what it needs, the agent copies in a few files and undoes the copies when it cleans up; see [Helper files and environment](environment.md). To bring back a file the task made, such as an export or a PDF:

```sh
atlas collect atlas-work/report.pdf
```

The reply gives the path of the copy on your side. Nothing else comes back.

Some places are readable by every account on your Mac, the helper included: `/Users/Shared`, most external drives, a disk image mounted in either account, and folders in your home that aren't private. Your Desktop, Documents, Downloads and Library are private; a folder you created yourself, such as `~/code`, usually isn't.

**Keep your home folder private from the helper.** Setup's last screen has this switch, on unless you turned it off before, and **Settings › Privacy › Your Files** shows how your home folder stands, with **Make Private** or **Undo…**. It adds one access entry to your home folder that keeps the helper, and only the helper, from listing it or going into it. Nothing else changes: your apps and other accounts keep their access, and `/Users/Shared` and external drives keep their own permissions. `atlas doctor` in your terminal says how many of your folders the helper can open while it isn't private. Only you open it again; an agent can make it private, never undo it.

While your home folder is private:

- Agents still open your apps and files from it: Atlas copies each one in first, and keeps the copy for 7 days. Rebuilding an app that is still open in the helper can stop at "An earlier copy … is still running"; close it there first.
- `atlas open` of a folder in your home, and a profile's `[[references]]` to a place in your home, stop working. Put shared material in `/Users/Shared`, or open a single file.
- An app that loads files by a full path into your home, such as a build that links a library from `~/dev`, can't reach them from the helper.

To undo it without the app, run `chmod -a "user:HELPER deny list,search" ~` with the helper's account name. Uninstalling Atlas leaves it in place and prints that line.

To make only one folder private instead:

```sh
chmod go-rwx ~/code
```

Atlas still opens builds from a private folder: it copies them for the helper.

Atlas limits what the helper's apps can see. Your agent itself runs in your account and can read what you can, as it does without Atlas.

### Signing the helper into accounts

Atlas never copies your sign-ins into the helper. It refuses to copy, or open there, browser profiles and cookies, keychains, the sign-in storage of apps such as Slack and Discord, SSH, GPG and cloud keys, `.env` and `.npmrc`. The refusal says what the file is. When a task needs an account, sign the helper in to it yourself.

Anything the helper is signed in to, an agent can use: it can read, send, merge or delete there as you could. So sign in only to what the task needs, and prefer a test account or one with few rights. Your agent's own app may also ask you before it runs a command; that is separate from Atlas.

**Sign In to a Site…** in **Settings › Accounts** opens the site in the helper's Google Chrome, or in Firefox when Chrome isn't installed, and gives you control there. Atlas starts it only when no agent's command or background job is running. Until you finish, agents are paused and can't see the helper's screen: their screenshots, element and window reads, the live picture in their apps and the pictures of their actions all answer `busy`, held by you. Sign in as usual; switching to your password manager or covering the viewer keeps control with you. Choose **Done** and the site joins your accounts: a new one starts at Ask Before Sending, so agents ask you before sending there, and one already listed keeps the level you gave it. **Cancel** adds nothing. Either way agents go on as before, and a Stop you chose meanwhile stays.

An account's **Forget…** and **Reset…** work the same way. Forget opens the browser's settings for that site's data, and Reset its page for clearing all of its data. Delete it there and choose **Done**. Atlas can't see what you deleted, so it then asks whether to remove the account from the list too.

Atlas checks where a file is, not what is in it: a renamed copy of a store, or a project folder copied whole with a `.env` inside, is not recognized.

If you want agents to copy such files in anyway, turn on **Copy sign-ins into the helper** in **Settings › Privacy › Sign-Ins**. It is off unless you turn it on, it asks first, and only you can turn it on; anyone can turn it off. Like Stop, it holds against agents using Atlas's commands, not against a shell that edits Atlas's files itself.

## Settings and practical limits

Open **Settings** from the menu bar, or click the helper's line at the foot of the main window's sidebar to open it on **Helper**. Its pages are:

- **Helper**: the helper account, whether it is ready to act with **Check Again**, and **Recover…** and **Log Out…** for when something goes wrong.
- **General**: opening Atlas at login, the menu bar's icon, how often the helper's screen updates, muting the helper, and notifications.
- **Activity**: keeping pictures of agent actions, how long, how much typed text beside them, and the pictures stored on this Mac with **Delete All…**.
- **Accounts**: the accounts agents act in, each with its own page for how freely they act there and its sign-in actions. An app shows its own icon; a site shows its icon, which Atlas reads from the site itself, without your cookies or sign-in, and keeps for 30 days.
- **Privacy**: keeping your home folder private from the helper, and whether agents may copy sign-ins in.
- **Integrations**: connecting coding agents, and the projects whose `AGENTS.md` has Atlas's section.
- **About**: the version, software updates and **Getting Started**.

Keep these limits in mind when choosing a task:

- Open apps with `atlas open`. A click on a Dock icon launches that app, so `--app` and `--pid` refuse a click, scroll or drag on the Dock. Dragging works for window movement, sliders and text selection, but does not provide native file or item drag and drop. With `--app` or `--window`, a drag may drop on the empty desktop only when it starts on the window's title bar and so moves the window; a drop on a desktop widget, the Dock or another window is still refused.
- Native Save and Open panels can be inert without authenticated Screen Sharing. Compact `doctor` warns; `capabilities.nativeDialogs` in `doctor --verbose` is dated evidence, not connection detection, and `ready` only means basic readiness. Cancel an unqualified panel with Escape, create files through `atlas run`, and open files directly with `atlas open --app TextEdit PATH`. Use a new filename in the helper's home to avoid overwriting work. Cmd+S can save an existing writable file in place; verify its bytes through `run`.
- Use Google Chrome or Firefox for hidden browser work, for example `atlas open --app "Google Chrome" http://localhost:3000` with your local server running. Safari and other WebKit views can remain blank without Screen Sharing. `capabilities.webContent` in `doctor --verbose` reports this constraint. Opening a URL without `--app` uses the default browser.
- Save working files in the helper's home rather than an app copy staged under `/Users/Shared/Atlas/Open`, which the helper can only read. [What the helper shares](#what-the-helper-shares) covers files, sound and other apps.
- The helper shares your Mac's process list. Use `atlas close --app NAME` instead of a broad process-killing command.

Coordinate any lock, sleep, logout or user switch with the person using the Mac. These transitions can interrupt the helper's session and change whether input is allowed.
