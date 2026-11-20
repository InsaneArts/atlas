# Desktops

Atlas runs agents' apps on a desktop of its own, so you keep using yours. A desktop is one of two kinds:

| Desktop      | What it is                                                                                                                                                    | Kind in the settings |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------- |
| Second Login | A hidden second macOS account on this Mac, called the helper. Setup in Atlas.app makes it. See [Getting started](getting-started.md).                         | `user`               |
| VM           | A macOS virtual machine on this Mac, with its own disk, account and screen. It shares nothing with your Mac but the `localhost` servers Atlas passes into it. | `vm`                 |

You can have one Second Login and several VMs. Agents use the same commands on both. A VM keeps agents further from your Mac: nothing of your account is there unless an agent copies it in. Second Login shares your Mac's network and speakers, and can be given copies of your sign-ins.

## Choose a desktop

Every command uses one desktop. Without a choice, it uses the settings' default. To choose another:

- `--desktop NAME` for one command, for example `atlas --desktop work screenshot`;
- `ATLAS_DESKTOP=NAME` for every command it reaches, such as an agent host started with it.

`atlas desktop list` shows each desktop, its kind and which is the default. With more than one desktop, every reply names the desktop it is for in a `desktop` field, and agents are told to act only on the one their reply names.

The settings list desktops under `[[desktops]]`, with a `default`:

```toml
default = "second-login"

[[desktops]]
name = "second-login"
kind = "user"
helper_uid = 502
socket_path = "/Users/helper/.atlas/helper/502.sock"
identity_probe = "/Users/Shared/Atlas/atlas"
input_socket = "/Users/helper/.atlas/input/input.sock"

[[desktops]]
name = "work"
kind = "vm"
id = "0123456789abcdef01234567"
```

`atlas desktop add vm` writes a VM's entry, with the identifier Atlas VM gave it. Change `default` to make another desktop the default. Settings that name one helper at the top, as setup writes them, are one Second Login called `second-login`. [Configuration](configuration.md#desktops) has the rules.

Atlas.app shows one desktop at a time, the default until you pick another. With more than one set up, click the row at the foot of the main window's sidebar, or the desktop's name beside **⋯** in the menu bar panel, and pick one: each is listed with its state. The live picture, **Take Control**, **Resume** and **Sign In to a Site…** are for the desktop shown. The sidebar lists the sessions working on every desktop; one on another desktop says which, as "On Second Login · Typing in Notes", and choosing it shows its Activity while the desktop shown stays. **Stop** stops agents on every desktop, and the menu bar shows the most urgent state of them all. Picking a desktop changes only what Atlas.app shows, never the default or the desktop an agent's command acts on.

## What a VM needs

- macOS 27 or later on this Mac, and Apple silicon.
- 70 GB free on the disk of your home folder, or 40 GB when Atlas has already downloaded macOS.
- A download of macOS from Apple, about 26 GB, the first time. Atlas keeps it for the next VM.

Each VM has 4 processors, 8 GB of memory while it runs, one 1920 × 1200 display and a 128 GB disk that uses only what the VM fills: about 28 GB once macOS is installed.

macOS runs at most two macOS VMs at once on one Mac, counting other apps' VMs. Atlas does not make or start a third one; stop a VM first.

macOS's licence allows macOS VMs for developing and testing software. A VM desktop is for that.

## Add a VM

From your own account, run:

```sh
atlas desktop add vm --name work
```

Without `--name`, the VM is called `vm`, then `vm-2` and so on. Adding takes a while:

1. Atlas.app asks you to allow it. The request says which app asked and that Atlas downloads macOS from Apple, makes a VM and opens its window once. Allow it only if you asked for this.
2. Atlas VM checks this Mac, downloads macOS, installs it and starts the VM, then puts Atlas in it. `atlas desktop list --json` shows the progress.
3. The VM's window opens on your screen. In it, allow **Screen & System Audio Recording**, then **Accessibility**, for Atlas, as macOS asks. Atlas in the VM restarts once after the first. If macOS asks for an administrator, the VM's window shows the name and password: choose **Show** to see the password, and type it without spaces. Atlas never types it, and it is shown nowhere else. `atlas --desktop work status` says what it waits for.
4. Once both are allowed, the window stops taking your keyboard and mouse, and the add lets agents go on in the VM itself, under the same approval: "Setup is done. Agents can use the VM now."

If you stopped agents meanwhile, or the VM's Atlas waits for your Resume for another reason, the add says instead: "Setup is done. Resume agents from the Atlas menu bar to let them use the VM." Then choose **Resume** in the Atlas menu bar. `atlas desktop list --json` says where the setup stands in `setup`: `waiting` for the permissions (and `waiting` names them), `resuming`, then `done`.

If the add is cut short, run the same command again: it goes on from where it got, and a cut download resumes. If you close the VM's window before both permissions are allowed, the add stops; run it again to open the window again. The add waits for the permissions for about half an hour, then stops with the window open.

Atlas.app's Resume is for the desktop it shows. When the new VM isn't the default, make it the default to resume it, then change the default back if you like. Agents that name no desktop use the default too.

## Agents wait for your Resume

Agents never use a VM until it is resumed there. A new VM is resumed by its add once you allowed both permissions, unless you stopped agents; a VM waits again whenever its Atlas needs your Resume, for example after its Screen Recording changed. Until then, agents get `stopped` and wait. **Stop** in Atlas.app stops agents on every desktop; **Resume** lets them go on, on the desktop Atlas.app shows. Only you resume agents.

## Watch and control a VM

A VM's live picture and control are for Atlas.app alone. With the VM as the shown desktop, Atlas.app's viewer shows it live, and **Take Control** gives you its keyboard and mouse, even while agents are stopped. Take Control is refused while an agent's command runs there. Other programs, agents among them, cannot read a VM's live picture or take control of it. Agents still take their own screenshots. `atlas --desktop work monitor --terminal` shows a VM in a terminal pane, without control, and stops when the VM's screen is not for agents to see, such as during maintenance.

## Sign in on a VM

**Settings › Accounts › Sign In to a Site…** works on a VM as on Second Login. The VM needs Google Chrome or Firefox. Atlas gives you control, opens the site in the VM's browser and hides the VM's screen from agents until you choose **Done** or **Cancel** and your keyboard and mouse are let go there. The account is listed with the VM it was signed in on: the same site on Second Login and on a VM are two accounts.

**Copy sign-ins into the helper** in **Settings › Privacy › Sign-Ins** works only with Second Login. Copies into a VM never take browser profiles, keychains, app sign-ins, keys or tokens, whatever you chose there.

## Maintenance

To use the VM yourself, for example to install a macOS update or an app, open it for maintenance:

```sh
atlas desktop maintain work
```

Atlas.app asks you to allow it. Agents' work there stops, and the VM's window opens on your screen and takes your keyboard and mouse. Closing the window does not end maintenance. When you are done, end it:

```sh
atlas desktop maintain work --end
```

The window then takes no input, and agents may go on. Maintenance is refused while agents' work runs in the VM; `--restart` stops the VM and starts it again first, ending that work. `atlas desktop list --json` says the VM's macOS version and whether a newer one is out, under `macos`.

`atlas desktop start work` starts a stopped VM, unless agents are stopped. A VM also starts by itself when a command needs it.

## Atlas in the VM

`atlas update` brings Atlas in each VM to the same version as on your Mac. It stops agents on each VM it updates, and they stay stopped until you resume them. While agents' work runs on a VM, the update is refused unless you add `--allow-disrupt`. An update is refused while a VM is open for maintenance: end it first. If a VM's update failed, `atlas update --retry-vms` tries it again. A VM whose Atlas can't be updated from this Mac says so in `atlas --desktop work status`: remove it and add it again.

## Servers on `localhost`

Servers you run on this Mac's `localhost` (`127.0.0.1` and `::1`) in your own account can be reached at the same ports on the VM's `localhost`, so an app in the VM can use your development server. Atlas passes up to 32 ports, lowest first, and only connections to the very program that listens there. Atlas's own and macOS's own servers are never passed. A port the VM uses itself is listed as in use in `atlas --desktop work status`. Maintenance closes them until it ends.

## What a VM doesn't have

- **No App Store sign-in.** macOS doesn't allow it in a VM, so apps that need it won't run there.
- **No sound.** A VM has no sound device, so its apps make no sound, and `atlas sound` says so.
- **No log-out.** `atlas logout` is for Second Login; a VM keeps its own login.

## Remove a VM

```sh
atlas desktop remove work
```

Atlas.app asks you to allow it. Atlas stops the VM and deletes it, its settings entry and what Atlas kept of it. When it was the default, the default moves to Second Login, or to the first desktop left. The last desktop in the settings can't be removed this way. A file Atlas finds there that was not the VM's own is kept and listed, never deleted; so is a Keychain item Atlas could not delete without asking. If the removal is cut short, run it again. The macOS download stays for the next VM.

`atlas uninstall` removes every VM first, once you allow it in Atlas.app. See [Maintenance](maintenance.md#uninstall).
