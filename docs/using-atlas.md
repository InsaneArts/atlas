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

The named click refuses missing or ambiguous targets. Inspect the second screenshot to check the result. A successful input response confirms delivery, not that the app did what you intended. Check the resulting screen, accessibility elements or saved file after each meaningful step.

If a control has no usable accessibility element, take a fresh screenshot and use `atlas click X Y` with the observed position. Coordinates are display points measured from the top-left corner. Convert screenshot pixel coordinates with the result's `origin` and `pointsPerPixel`: point = origin + pixel × pointsPerPixel. `atlas screenshot --app NAME` or `--region X,Y,W,H` takes a smaller picture of one app's windows or one part of the display. Do not reuse coordinates after the window or display changes.

When you finish, close this app:

```sh
atlas close --app Example
```

This quits the app and removes any copy Atlas staged for it. Close the previous build before opening a replacement. Use the [command reference](../agents/atlas/skills/atlas/references/commands.md) for typing, key combinations, scrolling and other input options.

## Watch the helper

Click the Atlas menu bar item for a small screen and current status. Choose **Open Atlas**, or run `atlas monitor`, for the full native viewer. It opens on your desktop while the apps you are testing stay in the helper.

The viewer has four tabs:

| Tab         | What you can see                        |
| ----------- | --------------------------------------- |
| Screen      | The helper's current picture.           |
| Activity    | Recent Atlas actions and their results. |
| Apps        | Apps running in the helper.             |
| Screenshots | Captures saved by Atlas.                |

When you leave Screen, a small screen remains over the other tabs. Drag or throw it toward another corner to move it out of the way. Click it to return to Screen. It follows the viewer's picture setting, including a paused picture.

You can choose **Live**, **Every 5 Seconds** or **Paused** for the picture. Pausing the picture does not stop work in the helper. The menu bar panel's pause control affects its own picture. In the full viewer, **Actual Size** shows one display point per view point and lets you scroll when the screen does not fit.

Some agent hosts can show Atlas inside the conversation. That is a different viewing route from the native window; see [Agent connections](agents.md) for host support.

## Take control or stop agents

Choose **Take Control** in the viewer to use your mouse and keyboard in the helper. Atlas waits for control to become available before accepting your input. While you hold control, agents cannot send competing input. Choose **Give Back**, or press **Control-Shift-Esc**, when you are done.

**Stop** cancels agent input underway and blocks further agent actions. It is separate from pausing the picture and from taking control. Use **Resume** when you want agents to act again. An agent must not resume itself after you stop it.

If Atlas reports that an action's outcome is unknown, do not send it again. Observe the helper if it is available, follow Atlas's recovery instructions, then inspect the result before continuing. If the helper is unavailable, complete the login or permission step first. Recovery does not prove that the earlier action did nothing. The [error reference](../agents/atlas/skills/atlas/references/troubleshooting.md) gives the next step for each error code.

## Settings and practical limits

Open **Settings** to change picture behavior in **General**, check the account and readiness in **Helper**, manage connections in **Agents**, or view version information in **About**.

Keep these limits in mind when choosing a task:

- Open apps with `atlas open`. A click on a Dock icon launches that app, so `--app` and `--pid` refuse a click, scroll or drag on the Dock. Dragging works for window movement, sliders and text selection, but does not provide native file or item drag and drop.
- Native Save and Open panels need an authenticated Screen Sharing connection to the helper. Check `capabilities.nativeDialogs` in `doctor --verbose`; a delivered click alone does not prove a dialog accepted it.
- Use Google Chrome or Firefox for hidden browser work, for example `atlas open --app "Google Chrome" http://localhost:3000` with your local server running. Safari and other WebKit views can remain blank without Screen Sharing. `capabilities.webContent` in `doctor --verbose` reports this constraint. Opening a URL without `--app` uses the default browser.
- The helper has its own files and sign-ins. Atlas does not copy your credentials. Save working files in the helper's home rather than an app copy staged under `/Users/Shared/Atlas/Open`, which the helper can only read.
- The helper shares your Mac's network, speakers and process list. Audio can play through your speakers. Use `atlas close --app NAME` instead of a broad process-killing command.

Coordinate any lock, sleep, logout or user switch with the person using the Mac. These transitions can interrupt the helper's session and change whether input is allowed.
