#if DEBUG
  /// The setup window's account samples: choosing an account to be the helper and creating one, with the accounts
  /// and forms they show.
  extension PreviewStates {
    /// The account choice, a new account picked.
    @MainActor package static let onboardingChoose = OnboardingState(
      content: .screen(.choose(onboardingChooseScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .continue, event: .continueChoice, isEnabled: true, isDefault: true),
        dots: .init(index: 1, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The account form, its passwords not typed yet.
    @MainActor package static let onboardingCreate = OnboardingState(
      content: .screen(.create(onboardingCreateScreen)),
      footer: OnboardingFooterState(
        back: onboardingBack,
        next: OnboardingFooterAction(title: .createAccount, event: .createAccount, isEnabled: false, isDefault: true),
        dots: .init(index: 1, count: 6)
      ),
      note: .none,
      isBusy: false
    )

    /// The account form with a short name macOS refuses: the row is marked, the note says why and what to try, and
    /// Create waits. Drawn with ``onboardingFormBadName``.
    @MainActor package static let onboardingCreateBadName: OnboardingState = {
      var state = onboardingCreate
      state.content = .screen(
        .create(
          OnboardingCreateScreenState(
            tail: "You'll approve it once in macOS.",
            note: .problem("Account names can't have spaces. Try “atlas-helper”."),
            isFormEnabled: true,
            flagged: .accountName
          )
        )
      )
      return state
    }()

    /// The account being made: the form and the footer wait for macOS's dialog.
    @MainActor package static let onboardingCreateWorking: OnboardingState = {
      var state = onboardingCreate
      state.content = .screen(
        .create(
          OnboardingCreateScreenState(
            tail: "You'll approve it once in macOS.",
            note: .working(
              "Approve in the macOS dialog. It says Atlas can change your Mac. "
                + "That covers only the three steps above."
            ),
            isFormEnabled: false
          )
        )
      )
      state.footer.back?.isEnabled = false
      state.note = .working(
        "Approve in the macOS dialog. It says Atlas can change your Mac. "
          + "That covers only the three steps above."
      )
      state.isBusy = true
      return state
    }()

    /// The account choice with more standard accounts than the window holds: the rows scroll, the footer stays.
    @MainActor package static let onboardingChooseMany: OnboardingState = {
      var state = onboardingChoose
      state.content = .screen(
        .choose(
          OnboardingChooseScreenState(
            isNewAccountPicked: true,
            accounts: [
              onboardingAccount("Test Account", "testaccount"),
              onboardingAccount("Guest Kid", "kid"),
              onboardingAccount("Maria Fernanda Montgomery-Castellanos de la Vega", "maria"),
              onboardingAccount("Studio", "studio"),
            ]
          )
        )
      )
      return state
    }()

    /// The account choice with an account whose name is longer than two lines, picked: its row cuts the name in its
    /// middle.
    @MainActor package static let onboardingChooseLong: OnboardingState = {
      var state = onboardingChoose
      var long = onboardingAccount(
        "Maria Fernanda Montgomery-Castellanos de la Vega y Rodríguez-Hernández del Valle Alto de San Sebastián y Monteverde de los Ríos",
        "mariafernanda"
      )
      long.isSelected = true
      state.content = .screen(
        .choose(
          OnboardingChooseScreenState(
            isNewAccountPicked: false,
            accounts: [long, onboardingAccount("Test Account", "testaccount")],
            approval:
              "You'll approve once in macOS so Atlas can get Maria Fernanda Montgomery-Castellanos de la Vega y Rodríguez-Hernández del Valle Alto de San Sebastián y Monteverde de los Ríos ready: Atlas opens there by itself, and an account never used yet skips macOS's welcome questions. Nothing in the account is removed or replaced."
          )
        )
      )
      return state
    }()

    /// The account choice with two standard accounts, and why the last choice failed.
    @MainActor package static let onboardingChooseProblem: OnboardingState = {
      var state = onboardingChoose
      state.content = .screen(
        .choose(
          OnboardingChooseScreenState(
            isNewAccountPicked: true,
            accounts: [onboardingAccount("Test Account", "testaccount"), onboardingAccount("Guest Kid", "kid")]
          )
        )
      )
      state.note = .problem("macOS could not say whether kid is an administrator.")
      return state
    }()

    /// Atlas was set up in the account chosen before, so that account is the one choice.
    @MainActor package static let onboardingChooseOnly: OnboardingState = {
      var state = onboardingChoose
      state.content = .screen(
        .choose(
          OnboardingChooseScreenState(
            isNewAccountPicked: false,
            accounts: [
              OnboardingAccountChoice(
                id: .existing("atlas"),
                title: "Atlas",
                detail: "atlas · standard account on this Mac",
                mark: .letter("A"),
                isSelected: true
              )
            ],
            offersNewAccount: false
          )
        )
      )
      state.footer.back = nil
      return state
    }()

    /// A standard account in the choice, not picked.
    ///
    /// - Parameters:
    ///   - title: Its full name.
    ///   - name: Its short name.
    /// - Returns: The account's row.
    @MainActor static func onboardingAccount(_ title: String, _ name: String) -> OnboardingAccountChoice {
      OnboardingAccountChoice(
        id: .existing(name),
        title: title,
        detail: "\(name) · standard account on this Mac",
        mark: .letter(String(title.prefix(1))),
        isSelected: false
      )
    }

    /// The form a new window opens with.
    @MainActor package static let onboardingForm = OnboardingAccountForm()

    /// A filled form whose short name has a space, which macOS refuses.
    @MainActor package static let onboardingFormBadName = OnboardingAccountForm(
      fullName: "Atlas Helper",
      accountName: "Atlas Helper",
      password: "secret",
      verify: "secret"
    )

    /// A new account picked over one standard account.
    @MainActor static let onboardingChooseScreen = OnboardingChooseScreenState(
      isNewAccountPicked: true,
      accounts: [
        OnboardingAccountChoice(
          id: .existing("testaccount"),
          title: "Test Account",
          detail: "testaccount · standard account on this Mac",
          mark: .letter("T"),
          isSelected: false
        )
      ]
    )

    /// The form ready to fill.
    @MainActor static let onboardingCreateScreen = OnboardingCreateScreenState(
      tail: "You'll approve it once in macOS.",
      note: .none,
      isFormEnabled: true
    )
  }
#endif
