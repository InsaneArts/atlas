# Setup once, reuse the desktop

The investigation's many account switches were troubleshooting and qualification,
not the intended per-task workflow. On the original Mac, setup is already done:
keep the helper logged in, run `understudy doctor`, then start a task or viewer.
The helper itself stays at `/Users/Shared/AtlasLiveTest/offstage-sessiond` so its
existing signature/path and normal permissions are preserved.

| Step | Current behavior | Intended improvement |
| --- | --- | --- |
| Account | One prepared standard account is reused | An installer finds an existing helper first; creates one only when needed, with administrator authorization |
| Helper files and launch | Existing LaunchAgent restarts the helper; no manual Terminal per task | Install a consistently signed helper and LaunchAgent through one guided setup |
| First graphical login | User logs into the account and completes macOS setup | Guide one first-login pass; don't confuse changing a process UID with creating an Aqua desktop |
| Privacy consent | Normal Screen Recording and Accessibility consent | Show exact outstanding permissions, preserve signed identity across updates, avoid repeated prompts |
| Readiness | `understudy doctor` works now | Integrate the same state into an onboarding UI |
| Watching and controlling work | `understudy monitor` provides a live preview and explicit mouse/keyboard control | Optional menu-bar viewer using the same capture path |
| New task | Reuse the account and serialize its UI work | Separate task folders/profiles; don't create a macOS account per job |
| Reboot/logout | Not qualified; graphical login must be restored | Explicitly test and design session provisioning before promising zero-touch startup |

The installer and menu-bar app in this table are proposed work, not shipped
features. The command-line client, readiness check and interactive browser viewer are implemented.

Apple supports multiple simultaneously logged-in users through
[Fast User Switching](https://support.apple.com/guide/mac-help/switch-quickly-between-users-mchlp2439/mac).
Normal capture permission is managed in
[Screen & System Audio Recording](https://support.apple.com/guide/mac-help/control-access-screen-system-audio-recording-mchld6aa7d23/mac).
Apple also states that [automatic login is unavailable with FileVault enabled](https://support.apple.com/en-us/102316).
Disabling FileVault or writing privacy databases is not this project's setup strategy.

Removing the second account altogether changes the architecture. A macOS VM is
a possible independent desktop, but brings its own OS provisioning and consent;
no VM desktop has been qualified by this project. Another Space or hidden window
in the main account does not provide the session separation measured here.

One account has one UI focus. Queue its control work until service-enforced
ownership is implemented; the current Python lock coordinates only cooperating
Understudy clients. Observation can run concurrently; the viewer's explicit
control mode holds the same lock as CLI mutations.
