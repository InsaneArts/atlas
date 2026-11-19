#if DEBUG
  extension PreviewStates {
    /// The sidebar with two desktops: its first row says Desktops, all ready.
    static let settingsDesktopsSidebar: SettingsSidebarState = {
      var sidebar = settingsSidebar
      sidebar.rows[0] = SettingsSidebarRow(page: .helper, title: "Desktops", subtitle: "Ready", tone: .ready)
      return sidebar
    }()

    /// Second Login, the default, and the VM work, running.
    static let settingsDesktopsPane = SettingsDesktopsState(
      rows: [
        SettingsDesktopRow(
          id: "user-id",
          title: "Second Login",
          detail: "A hidden second login on this Mac",
          isDefault: true,
          isVM: false
        ),
        SettingsDesktopRow(id: "vm-id", title: "work", detail: "A virtual Mac on this Mac · Running", isVM: true),
      ]
    )

    /// work's page while it runs: Stop…, Open…, its disk, a newer macOS, its Atlas up to date, and Remove….
    static let settingsVMPane = SettingsVMState(
      id: "vm-id",
      title: "work",
      line: "A virtual Mac on this Mac · 1920 × 1200",
      tone: .ready,
      sections: [
        SettingsVMSection(
          title: "Power and Maintenance",
          rows: [
            SettingsVMRow(
              id: "power",
              title: "Running",
              detail: "Starts by itself when an agent needs it",
              button: SettingsVMButton(title: "Stop…", event: .askStopVM("vm-id"))
            ),
            SettingsVMRow(
              id: "maintenance",
              title: "Open for maintenance",
              detail: "Use the VM yourself, such as to update macOS. Agents wait.",
              button: SettingsVMButton(title: "Open…", event: .maintainVM("vm-id"))
            ),
          ]
        ),
        SettingsVMSection(
          title: "Disk and Software",
          rows: [
            SettingsVMRow(id: "disk", title: "Disk", value: "28 GB used of 128 GB"),
            SettingsVMRow(
              id: "macos",
              title: "macOS 27.2.1",
              detail: "macOS 27.3 is available. Open the VM for maintenance and update it in Software Update."
            ),
            SettingsVMRow(id: "atlas", title: "Atlas in the VM", value: "Up to date"),
          ]
        ),
        SettingsVMSection(
          rows: [
            SettingsVMRow(
              id: "remove",
              title: "Remove this VM",
              detail: "Deletes the VM and everything in it",
              button: SettingsVMButton(title: "Remove…", event: .removeVM("vm-id"), isDestructive: true)
            )
          ]
        ),
      ]
    )

    /// Settings with two desktops, on Desktops.
    @MainActor static let settingsDesktops: SettingsState = {
      var state = selecting(.helper)
      state.sidebar = settingsDesktopsSidebar
      state.desktops = settingsDesktopsPane
      return state
    }()

    /// The Helper page while Second Login is the one desktop: Add a Desktop… at its foot.
    @MainActor static let settingsHelperAddsDesktop: SettingsState = {
      var state = selecting(.helper)
      state.helper.addsDesktop = true
      return state
    }()

    /// work's page, opened from Desktops, while it runs.
    @MainActor static let settingsVM: SettingsState = {
      var state = settingsDesktops
      state.route = .desktop("vm-id")
      state.vm = settingsVMPane
      return state
    }()

    /// work's page while the person has it open for maintenance: End…, Stop… waiting, the dot blue.
    @MainActor static let settingsVMMaintenance: SettingsState = {
      var state = settingsVM
      var vm = settingsVMPane
      vm.tone = .link
      vm.sections[0].rows[0].button?.isEnabled = false
      vm.sections[0].rows[1] = SettingsVMRow(
        id: "maintenance",
        title: "Open for maintenance",
        detail: "Agents wait until you end it",
        button: SettingsVMButton(title: "End…", event: .endMaintenance("vm-id"))
      )
      state.vm = vm
      return state
    }()

    /// Stop… asked on work's page.
    @MainActor static let settingsStopVM: SettingsState = {
      var state = settingsVM
      state.alert = AlertCardState(
        title: "Stop work?",
        message: "Anything agents are doing there stops. It starts again when an agent needs it.",
        buttons: [
          AlertCardButton(id: .cancel, title: "Cancel", role: .safeCancel),
          AlertCardButton(id: .confirm, title: "Stop", role: .destructive),
        ]
      )
      return state
    }()
  }
#endif
