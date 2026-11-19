#if DEBUG
  /// The setup window's samples of the choice of where agents work and of the VM path: the choice with and without a
  /// VM, the download, the install, the permissions, a stopped and a failed add, the VM's check and Done.
  extension PreviewStates {
    /// The choice on a Mac that can run a VM: Second Login picked as it opens.
    @MainActor public static let onboardingDesktop = OnboardingState(
      content: .screen(.desktop(onboardingDesktopScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .continue, event: .continueDesktop, isEnabled: true, isDefault: true),
        track: onboardingTrack(1, of: 7)
      ),
      note: .none
    )

    /// The choice on a Mac without room for a VM: VM dimmed, saying why.
    @MainActor public static let onboardingDesktopNoVM: OnboardingState = {
      var state = onboardingDesktop
      state.content = .screen(.desktop(onboardingDesktopNoVMScreen))
      return state
    }()

    /// macOS coming down: Continue waits until the VM is made.
    @MainActor public static let onboardingVMDownload = OnboardingState(
      content: .screen(.vmDownload(onboardingVMDownloadScreen)),
      footer: onboardingVMFooter(2),
      note: .none
    )

    /// Atlas making the VM, the VM starting.
    @MainActor public static let onboardingVMInstall = OnboardingState(
      content: .screen(.vmInstall(onboardingVMInstallScreen)),
      footer: onboardingVMFooter(3),
      note: .none
    )

    /// The person allowing Atlas in the VM's window, Screen & System Audio Recording allowed.
    @MainActor public static let onboardingVMAllow = OnboardingState(
      content: .screen(.vmAllow(onboardingVMAllowScreen)),
      footer: onboardingVMFooter(4),
      note: .none
    )

    /// The add's process ended mid-download: the meter held, and Go On.
    @MainActor public static let onboardingVMStopped = OnboardingState(
      content: .screen(.vmDownload(onboardingVMStoppedScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .goOn, event: .goOnAdding, isEnabled: true, isDefault: true),
        track: onboardingTrack(2, of: 8)
      ),
      note: .none
    )

    /// The add failed making the VM: Atlas VM's reason, and Try Again.
    @MainActor public static let onboardingVMFailed = OnboardingState(
      content: .screen(.vmInstall(onboardingVMFailedScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .tryAgain, event: .retryAdding, isEnabled: true, isDefault: true),
        track: onboardingTrack(3, of: 8)
      ),
      note: .none
    )

    /// Atlas checking the VM works.
    @MainActor public static let onboardingVMFinishing = OnboardingState(
      content: .screen(.finishing(onboardingVMFinishingScreen)),
      footer: OnboardingFooterState(
        next: OnboardingFooterAction(title: .continue, event: .continueFinishing, isEnabled: false, isDefault: false),
        track: onboardingTrack(5, of: 8)
      ),
      note: .none
    )

    /// The VM's check passed: Continue opens.
    @MainActor public static let onboardingVMFinishingPassed: OnboardingState = {
      var state = onboardingVMFinishing
      var screen = onboardingVMFinishingScreen
      screen.title = "Everything works."
      screen.tail = "Your agents have a Mac now."
      screen.live = .done
      screen.liveNote = "Passed"
      state.content = .screen(.finishing(screen))
      state.footer.next?.isEnabled = true
      state.footer.next?.isDefault = true
      return state
    }()

    /// Setup done with a VM: no home folder to keep private.
    @MainActor public static let onboardingVMDone = OnboardingState(
      content: .screen(
        .done(
          OnboardingDoneScreenState(
            lede:
              "Atlas lives in your menu bar. Ask your agent to check something on screen and watch it work there.",
            opensAtLogin: true,
            offersHomePrivacy: false
          )
        )
      ),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .done, event: .complete, isEnabled: true, isDefault: true),
        track: onboardingTrack(7, of: 8)
      ),
      note: .none
    )

    /// The choice's rows on a Mac that can run a VM: Second Login picked, VM a choice, Cloud not yet.
    @MainActor static let onboardingDesktopScreen = OnboardingDesktopScreenState(
      items: [
        OnboardingDesktopItem(id: .secondLogin, standing: .choice(isPicked: true)),
        OnboardingDesktopItem(id: .vm, standing: .choice(isPicked: false)),
        OnboardingDesktopItem(id: .cloud, standing: .notYet("Not available yet")),
      ]
    )

    /// The choice's rows on a Mac without room for a VM.
    @MainActor static let onboardingDesktopNoVMScreen = OnboardingDesktopScreenState(
      items: [
        OnboardingDesktopItem(id: .secondLogin, standing: .choice(isPicked: true)),
        OnboardingDesktopItem(id: .vm, standing: .notYet("Needs 70 GB free")),
        OnboardingDesktopItem(id: .cloud, standing: .notYet("Not available yet")),
      ]
    )

    /// 12.4 GB of 25.6 GB down.
    @MainActor static let onboardingVMDownloadScreen = OnboardingVMDownloadScreenState(
      fraction: 12.4 / 25.6,
      amount: "12.4 GB of 25.6 GB"
    )

    /// The download held where it was, its process ended.
    @MainActor static let onboardingVMStoppedScreen = OnboardingVMDownloadScreenState(
      fraction: 12.4 / 25.6,
      amount: "12.4 GB of 25.6 GB",
      isHeld: true,
      problem: "Setting up the VM stopped."
    )

    /// macOS installed, the VM starting.
    @MainActor static let onboardingVMInstallScreen = OnboardingVMInstallScreenState(
      steps: [.done, .current, .waiting, .waiting]
    )

    /// Making the VM failed at its first step.
    @MainActor static let onboardingVMFailedScreen = OnboardingVMInstallScreenState(
      steps: [.current, .waiting, .waiting, .waiting],
      problem: "Atlas couldn’t make the VM: the VM’s disk could not be made."
    )

    /// Screen & System Audio Recording allowed, Accessibility next.
    @MainActor static let onboardingVMAllowScreen = OnboardingVMAllowScreenState(steps: [.done, .current])

    /// The VM's check under way.
    @MainActor static let onboardingVMFinishingScreen = OnboardingFinishingScreenState(
      title: "Checking everything",
      tail: "end to end.",
      loggedIn: "",
      permissionsNote: "Allowed",
      live: .busy,
      liveNote: "Checking…",
      isVM: true
    )

    /// The footer while the VM's add works: Continue dimmed, at `index` of the VM path's eight.
    ///
    /// - Parameter index: The screen, from 0.
    /// - Returns: The footer.
    @MainActor static func onboardingVMFooter(_ index: Int) -> OnboardingFooterState {
      OnboardingFooterState(
        next: OnboardingFooterAction(title: .continue, event: .continueFinishing, isEnabled: false, isDefault: false),
        track: onboardingTrack(index, of: 8)
      )
    }
  }
#endif
